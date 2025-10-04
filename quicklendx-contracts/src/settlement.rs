use crate::audit::log_payment_processed;
use crate::errors::QuickLendXError;
use crate::events::{
    emit_automated_settlement_triggered, emit_invoice_settled, emit_partial_payment,
    emit_payment_detected, emit_payment_validation_failed, emit_settlement_queued,
    emit_settlement_retry,
};
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use crate::notifications::NotificationSystem;
use crate::payments::transfer_funds;
use crate::profits::calculate_profit;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec};

/// Payment event structure for automated detection
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentEvent {
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub transaction_id: String,
    pub source: String, // External payment source identifier
    pub timestamp: u64,
    pub currency: Address,
}

/// Settlement queue item for processing multiple payments
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementQueueItem {
    pub queue_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub payment_amount: i128,
    pub priority: u32, // Higher number = higher priority
    pub created_at: u64,
    pub retry_count: u32,
    pub max_retries: u32,
    pub status: SettlementStatus,
}

/// Settlement status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SettlementStatus {
    Pending,    // Waiting to be processed
    Processing, // Currently being processed
    Completed,  // Successfully completed
    Failed,     // Failed and needs retry or manual intervention
    Cancelled,  // Cancelled (e.g., duplicate payment)
}

/// Settlement queue storage
pub struct SettlementQueueStorage;

impl SettlementQueueStorage {
    const QUEUE_KEY: Symbol = symbol_short!("SETTLE_Q");
    const PROCESSED_KEY: Symbol = symbol_short!("PROCESSED");
    const MAX_QUEUE_SIZE: u32 = 1000;
    const MAX_RETRIES: u32 = 3;

    /// Add item to settlement queue
    pub fn enqueue(
        env: &Env,
        invoice_id: &BytesN<32>,
        payment_amount: i128,
        priority: u32,
    ) -> Result<BytesN<32>, QuickLendXError> {
        let queue_id = Self::generate_queue_id(env);
        
        // Check queue size limit
        let current_queue = Self::get_queue(env);
        if current_queue.len() >= Self::MAX_QUEUE_SIZE {
            return Err(QuickLendXError::SettlementQueueFull);
        }

        let item = SettlementQueueItem {
            queue_id: queue_id.clone(),
            invoice_id: invoice_id.clone(),
            payment_amount,
            priority,
            created_at: env.ledger().timestamp(),
            retry_count: 0,
            max_retries: Self::MAX_RETRIES,
            status: SettlementStatus::Pending,
        };

        let mut queue = current_queue;
        queue.push_back(item);
        env.storage().instance().set(&Self::QUEUE_KEY, &queue);

        Ok(queue_id)
    }

    /// Get settlement queue
    pub fn get_queue(env: &Env) -> Vec<SettlementQueueItem> {
        env.storage()
            .instance()
            .get(&Self::QUEUE_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Process next item in queue
    pub fn process_next(env: &Env) -> Result<Option<SettlementQueueItem>, QuickLendXError> {
        let mut queue = Self::get_queue(env);
        if queue.is_empty() {
            return Ok(None);
        }

        // Sort by priority (highest first) and creation time
        let mut items: Vec<SettlementQueueItem> = Vec::new(env);
        for item in queue.iter() {
            items.push_back(item.clone());
        }

        // Simple priority sorting (in a real implementation, you'd want more sophisticated sorting)
        let mut sorted_items: Vec<SettlementQueueItem> = Vec::new(env);
        while !items.is_empty() {
            let mut highest_priority_idx = 0;
            let mut highest_priority = 0u32;
            
            for (i, item) in items.iter().enumerate() {
                if item.priority > highest_priority {
                    highest_priority = item.priority;
                    highest_priority_idx = i;
                }
            }
            
            let item = items.get(highest_priority_idx as u32).unwrap();
            sorted_items.push_back(item.clone());
            items.remove(highest_priority_idx as u32);
        }

        // Process the highest priority item
        if let Some(mut item) = sorted_items.first() {
            item.status = SettlementStatus::Processing;
            
            // Remove from queue and update
            let mut updated_queue = Self::get_queue(env);
            updated_queue.remove(0); // Remove first item
            env.storage().instance().set(&Self::QUEUE_KEY, &updated_queue);

            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    /// Mark settlement as completed
    pub fn mark_completed(env: &Env, queue_id: &BytesN<32>) {
        let mut processed = Self::get_processed_settlements(env);
        processed.push_back(queue_id.clone());
        env.storage().instance().set(&Self::PROCESSED_KEY, &processed);
    }

    /// Mark settlement as failed and retry if possible
    pub fn mark_failed(env: &Env, queue_id: &BytesN<32>, reason: String) -> Result<bool, QuickLendXError> {
        // Check if we can retry
        let mut queue = Self::get_queue(env);
        for (i, mut item) in queue.iter().enumerate() {
            if item.queue_id == *queue_id {
                item.retry_count += 1;
                if item.retry_count < item.max_retries {
                    item.status = SettlementStatus::Pending;
                    queue.set(i as u32, item);
                    env.storage().instance().set(&Self::QUEUE_KEY, &queue);
                    return Ok(true); // Can retry
                } else {
                    item.status = SettlementStatus::Failed;
                    queue.set(i as u32, item);
                    env.storage().instance().set(&Self::QUEUE_KEY, &queue);
                    return Ok(false); // Cannot retry
                }
            }
        }
        Err(QuickLendXError::StorageKeyNotFound)
    }

    /// Get processed settlements
    pub fn get_processed_settlements(env: &Env) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&Self::PROCESSED_KEY)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Generate unique queue ID
    fn generate_queue_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let random: u64 = env.prng().gen_range(0..u64::MAX);
        BytesN::from_array(&env, &[
            (timestamp >> 56) as u8,
            (timestamp >> 48) as u8,
            (timestamp >> 40) as u8,
            (timestamp >> 32) as u8,
            (timestamp >> 24) as u8,
            (timestamp >> 16) as u8,
            (timestamp >> 8) as u8,
            timestamp as u8,
            (random >> 56) as u8,
            (random >> 48) as u8,
            (random >> 40) as u8,
            (random >> 32) as u8,
            (random >> 24) as u8,
            (random >> 16) as u8,
            (random >> 8) as u8,
            random as u8,
            (random >> 56) as u8,
            (random >> 48) as u8,
            (random >> 40) as u8,
            (random >> 32) as u8,
            (random >> 24) as u8,
            (random >> 16) as u8,
            (random >> 8) as u8,
            random as u8,
            (random >> 56) as u8,
            (random >> 48) as u8,
            (random >> 40) as u8,
            (random >> 32) as u8,
            (random >> 24) as u8,
            (random >> 16) as u8,
            (random >> 8) as u8,
            random as u8,
        ])
    }
}

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

/// Validate payment event for automated settlement
pub fn validate_payment_event(payment_event: &PaymentEvent) -> bool {
    // Basic validation checks
    if payment_event.amount <= 0 {
        return false;
    }
    
    if payment_event.transaction_id.len() == 0 {
        return false;
    }
    
    if payment_event.source.len() == 0 {
        return false;
    }
    
    // Check if timestamp is reasonable (not too far in the past or future)
    // For Soroban, we'll use a simpler validation - just check it's not zero
    if payment_event.timestamp == 0 {
        return false;
    }
    
    true
}

/// Detect payment and trigger automated settlement
pub fn detect_payment(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_event: PaymentEvent,
) -> Result<(), QuickLendXError> {
    // Validate payment event
    if !validate_payment_event(&payment_event) {
        emit_payment_validation_failed(
            env,
            invoice_id,
            payment_event.amount,
            &String::from_str(env, "Invalid payment event"),
        );
        return Err(QuickLendXError::InvalidPaymentEvent);
    }

    // Check if invoice exists and is in correct status
    let invoice = InvoiceStorage::get_invoice(env, invoice_id)
        .ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status != InvoiceStatus::Funded {
        emit_payment_validation_failed(
            env,
            invoice_id,
            payment_event.amount,
            &String::from_str(env, "Invoice not in funded status"),
        );
        return Err(QuickLendXError::InvalidStatus);
    }

    // Check if payment has already been processed
    let processed_settlements = SettlementQueueStorage::get_processed_settlements(env);
    for processed_id in processed_settlements.iter() {
        // In a real implementation, you'd check against transaction_id
        // For now, we'll just check if the invoice has been settled
        if invoice.status == InvoiceStatus::Paid {
            return Err(QuickLendXError::PaymentAlreadyProcessed);
        }
    }

    // Emit payment detected event
    emit_payment_detected(
        env,
        invoice_id,
        payment_event.amount,
        &payment_event.transaction_id,
        &payment_event.source,
    );

    // Add to settlement queue with priority based on amount
    let priority = if payment_event.amount >= invoice.amount {
        100 // High priority for full payments
    } else {
        50  // Medium priority for partial payments
    };

    let queue_id = SettlementQueueStorage::enqueue(
        env,
        invoice_id,
        payment_event.amount,
        priority,
    )?;

    emit_settlement_queued(env, invoice_id, &queue_id, priority);

    // Trigger automated settlement
    trigger_automated_settlement(env, invoice_id, payment_event.amount, &queue_id)?;

    Ok(())
}

/// Trigger automated settlement process
pub fn trigger_automated_settlement(
    env: &Env,
    invoice_id: &BytesN<32>,
    payment_amount: i128,
    settlement_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
    emit_automated_settlement_triggered(env, invoice_id, payment_amount, settlement_id);

    // Attempt to settle the invoice
    match settle_invoice(env, invoice_id, payment_amount) {
        Ok(()) => {
            // Mark settlement as completed
            SettlementQueueStorage::mark_completed(env, settlement_id);
            Ok(())
        }
        Err(e) => {
            // Mark settlement as failed and attempt retry
            let retry_reason = String::from_str(env, "Settlement failed");
            match SettlementQueueStorage::mark_failed(env, settlement_id, retry_reason) {
                Ok(can_retry) => {
                    if can_retry {
                        emit_settlement_retry(
                            env,
                            invoice_id,
                            settlement_id,
                            1, // This would be the actual retry count
                            &String::from_str(env, "Automatic retry"),
                        );
                        // In a real implementation, you might schedule a retry
                        // For now, we'll just return the original error
                    }
                    Err(e)
                }
                Err(_) => Err(e),
            }
        }
    }
}

/// Process settlement queue - can be called periodically
pub fn process_settlement_queue(env: &Env) -> Result<u32, QuickLendXError> {
    let mut processed_count = 0u32;
    let max_batch_size = 10; // Process up to 10 items per call

    for _ in 0..max_batch_size {
        match SettlementQueueStorage::process_next(env)? {
            Some(queue_item) => {
                match settle_invoice(env, &queue_item.invoice_id, queue_item.payment_amount) {
                    Ok(()) => {
                        SettlementQueueStorage::mark_completed(env, &queue_item.queue_id);
                        processed_count += 1;
                    }
                    Err(e) => {
                        let retry_reason = String::from_str(env, "Queue processing failed");
                        match SettlementQueueStorage::mark_failed(env, &queue_item.queue_id, retry_reason) {
                            Ok(can_retry) => {
                                if can_retry {
                                    emit_settlement_retry(
                                        env,
                                        &queue_item.invoice_id,
                                        &queue_item.queue_id,
                                        queue_item.retry_count + 1,
                                        &String::from_str(env, "Queue retry"),
                                    );
                                }
                            }
                            Err(_) => {}
                        }
                        // Continue processing other items even if one fails
                    }
                }
            }
            None => break, // No more items to process
        }
    }

    Ok(processed_count)
}

/// Get settlement queue status
pub fn get_settlement_queue_status(env: &Env) -> (u32, u32) {
    let queue = SettlementQueueStorage::get_queue(env);
    let processed = SettlementQueueStorage::get_processed_settlements(env);
    (queue.len() as u32, processed.len() as u32)
}

/// Retry failed settlements manually (admin function)
pub fn retry_failed_settlements(env: &Env, admin: &Address) -> Result<u32, QuickLendXError> {
    // In a real implementation, you'd verify admin authorization here
    admin.require_auth();

    let queue = SettlementQueueStorage::get_queue(env);
    let mut retry_count = 0u32;

    for mut item in queue.iter() {
        if item.status == SettlementStatus::Failed && item.retry_count < item.max_retries {
            item.status = SettlementStatus::Pending;
            item.retry_count += 1;
            
            // Update the item in storage (simplified - in real implementation you'd update properly)
            retry_count += 1;
            
            emit_settlement_retry(
                env,
                &item.invoice_id,
                &item.queue_id,
                item.retry_count,
                &String::from_str(env, "Manual retry"),
            );
        }
    }

    Ok(retry_count)
}
