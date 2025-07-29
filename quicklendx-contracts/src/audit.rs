use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};
use crate::invoice::{Invoice, InvoiceStatus};
use crate::errors::QuickLendXError;

/// Audit operation types
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuditOperation {
    InvoiceCreated,
    InvoiceUploaded,
    InvoiceVerified,
    InvoiceFunded,
    InvoicePaid,
    InvoiceDefaulted,
    InvoiceStatusChanged,
    InvoiceRated,
    BidPlaced,
    BidAccepted,
    BidWithdrawn,
    EscrowCreated,
    EscrowReleased,
    EscrowRefunded,
    PaymentProcessed,
    SettlementCompleted,
}

/// Audit log entry structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditLogEntry {
    pub audit_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub operation: AuditOperation,
    pub actor: Address,
    pub timestamp: u64,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub amount: Option<i128>,
    pub additional_data: Option<String>,
    pub block_height: u32,
    pub transaction_hash: Option<BytesN<32>>,
}

/// Audit query filters
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditQueryFilter {
    pub invoice_id: Option<BytesN<32>>,
    pub operation: Option<AuditOperation>,
    pub actor: Option<Address>,
    pub start_timestamp: Option<u64>,
    pub end_timestamp: Option<u64>,
}

/// Audit statistics
#[contracttype]
#[derive(Clone, Debug)]
pub struct AuditStats {
    pub total_entries: u32,
    pub operations_count: Vec<(AuditOperation, u32)>,
    pub unique_actors: u32,
    pub date_range: (u64, u64),
}

impl AuditLogEntry {
    /// Create a new audit log entry
    pub fn new(
        env: &Env,
        invoice_id: BytesN<32>,
        operation: AuditOperation,
        actor: Address,
        old_value: Option<String>,
        new_value: Option<String>,
        amount: Option<i128>,
        additional_data: Option<String>,
    ) -> Self {
        let audit_id = Self::generate_audit_id(env);
        let timestamp = env.ledger().timestamp();
        let block_height = env.ledger().sequence();
        
        Self {
            audit_id,
            invoice_id,
            operation,
            actor,
            timestamp,
            old_value,
            new_value,
            amount,
            additional_data,
            block_height,
            transaction_hash: None, // Could be populated if available
        }
    }

    /// Generate unique audit ID
    fn generate_audit_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        let counter_key = symbol_short!("aud_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        env.storage().instance().set(&counter_key, &(counter + 1));
        
        let mut id_bytes = [0u8; 32];
        // Add audit prefix
        id_bytes[0] = 0xAD; // 'A' for Audit
        id_bytes[1] = 0x1F; // 'U' for aUdit
        // Embed timestamp
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed sequence
        id_bytes[10..14].copy_from_slice(&sequence.to_be_bytes());
        // Embed counter
        id_bytes[14..22].copy_from_slice(&counter.to_be_bytes());
        // Fill remaining with pattern
        for i in 22..32 {
            id_bytes[i] = ((timestamp + sequence as u64 + counter + 0xAD1F) % 256) as u8;
        }
        BytesN::from_array(env, &id_bytes)
    }

    /// Validate audit log entry integrity
    pub fn validate_integrity(&self, env: &Env) -> Result<bool, QuickLendXError> {
        // Check timestamp is not in future
        if self.timestamp > env.ledger().timestamp() {
            return Ok(false);
        }
        
        // Check block height is valid
        if self.block_height > env.ledger().sequence() {
            return Ok(false);
        }
        
        // Validate operation-specific data
        match self.operation {
            AuditOperation::InvoiceFunded | AuditOperation::PaymentProcessed => {
                if self.amount.is_none() || self.amount.unwrap() <= 0 {
                    return Ok(false);
                }
            }
            AuditOperation::InvoiceStatusChanged => {
                if self.old_value.is_none() || self.new_value.is_none() {
                    return Ok(false);
                }
            }
            _ => {}
        }
        
        Ok(true)
    }
}

/// Audit storage and management
pub struct AuditStorage;

impl AuditStorage {
    /// Store an audit log entry
    pub fn store_audit_entry(env: &Env, entry: &AuditLogEntry) {
        // Store individual entry
        env.storage().instance().set(&entry.audit_id, entry);
        
        // Add to invoice audit trail
        Self::add_to_invoice_audit_trail(env, &entry.invoice_id, &entry.audit_id);
        
        // Add to operation index
        Self::add_to_operation_index(env, &entry.operation, &entry.audit_id);
        
        // Add to actor index
        Self::add_to_actor_index(env, &entry.actor, &entry.audit_id);
        
        // Add to timestamp index
        Self::add_to_timestamp_index(env, entry.timestamp, &entry.audit_id);
    }

    /// Get audit entry by ID
    pub fn get_audit_entry(env: &Env, audit_id: &BytesN<32>) -> Option<AuditLogEntry> {
        env.storage().instance().get(audit_id)
    }

    /// Get audit trail for an invoice
    pub fn get_invoice_audit_trail(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        let key = (symbol_short!("inv_aud"), invoice_id.clone());
        env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    /// Get audit entries by operation type
    pub fn get_audit_entries_by_operation(env: &Env, operation: &AuditOperation) -> Vec<BytesN<32>> {
        let key = (symbol_short!("op_aud"), operation.clone());
        env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    /// Get audit entries by actor
    pub fn get_audit_entries_by_actor(env: &Env, actor: &Address) -> Vec<BytesN<32>> {
        let key = (symbol_short!("act_aud"), actor.clone());
        env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    /// Query audit logs with filters
    pub fn query_audit_logs(env: &Env, filter: &AuditQueryFilter, limit: u32) -> Vec<AuditLogEntry> {
        let mut results = Vec::new(env);
        let mut count = 0u32;
        
        // Start with invoice-specific entries if invoice_id is provided
        let audit_ids = if let Some(invoice_id) = &filter.invoice_id {
            Self::get_invoice_audit_trail(env, invoice_id)
        } else if let Some(operation) = &filter.operation {
            Self::get_audit_entries_by_operation(env, operation)
        } else if let Some(actor) = &filter.actor {
            Self::get_audit_entries_by_actor(env, actor)
        } else {
            // Get all audit entries (expensive operation)
            Self::get_all_audit_entries(env)
        };
        
        for audit_id in audit_ids.iter() {
            if count >= limit {
                break;
            }
            
            if let Some(entry) = Self::get_audit_entry(env, &audit_id) {
                // Apply filters
                if Self::matches_filter(&entry, filter) {
                    results.push_back(entry);
                    count += 1;
                }
            }
        }
        
        results
    }

    /// Get audit statistics
    pub fn get_audit_stats(env: &Env) -> AuditStats {
        let all_entries = Self::get_all_audit_entries(env);
        let total_entries = all_entries.len() as u32;
        
        let mut operations_count = Vec::new(env);
        let mut unique_actors = Vec::new(env);
        let mut min_timestamp = u64::MAX;
        let mut max_timestamp = 0u64;
        
        for audit_id in all_entries.iter() {
            if let Some(entry) = Self::get_audit_entry(env, &audit_id) {
                // Track unique actors
                if !unique_actors.iter().any(|a| *a == entry.actor) {
                    unique_actors.push_back(entry.actor.clone());
                }
                
                // Update timestamp range
                if entry.timestamp < min_timestamp {
                    min_timestamp = entry.timestamp;
                }
                if entry.timestamp > max_timestamp {
                    max_timestamp = entry.timestamp;
                }
            }
        }
        
        AuditStats {
            total_entries,
            operations_count,
            unique_actors: unique_actors.len() as u32,
            date_range: (min_timestamp, max_timestamp),
        }
    }

    /// Validate audit log integrity for an invoice
    pub fn validate_invoice_audit_integrity(env: &Env, invoice_id: &BytesN<32>) -> Result<bool, QuickLendXError> {
        let audit_trail = Self::get_invoice_audit_trail(env, invoice_id);
        
        for audit_id in audit_trail.iter() {
            if let Some(entry) = Self::get_audit_entry(env, &audit_id) {
                if !entry.validate_integrity(env)? {
                    return Ok(false);
                }
            } else {
                return Ok(false); // Missing audit entry
            }
        }
        
        Ok(true)
    }

    // Helper methods
    fn add_to_invoice_audit_trail(env: &Env, invoice_id: &BytesN<32>, audit_id: &BytesN<32>) {
        let key = (symbol_short!("inv_aud"), invoice_id.clone());
        let mut trail = Self::get_invoice_audit_trail(env, invoice_id);
        trail.push_back(audit_id.clone());
        env.storage().instance().set(&key, &trail);
    }

    fn add_to_operation_index(env: &Env, operation: &AuditOperation, audit_id: &BytesN<32>) {
        let key = (symbol_short!("op_aud"), operation.clone());
        let mut entries = Self::get_audit_entries_by_operation(env, operation);
        entries.push_back(audit_id.clone());
        env.storage().instance().set(&key, &entries);
    }

    fn add_to_actor_index(env: &Env, actor: &Address, audit_id: &BytesN<32>) {
        let key = (symbol_short!("act_aud"), actor.clone());
        let mut entries = Self::get_audit_entries_by_actor(env, actor);
        entries.push_back(audit_id.clone());
        env.storage().instance().set(&key, &entries);
    }

    fn add_to_timestamp_index(env: &Env, timestamp: u64, audit_id: &BytesN<32>) {
        let day_key = timestamp / 86400; // Group by day
        let key = (symbol_short!("ts_aud"), day_key);
        let mut entries: Vec<BytesN<32>> = env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env));
        entries.push_back(audit_id.clone());
        env.storage().instance().set(&key, &entries);
    }

    fn get_all_audit_entries(env: &Env) -> Vec<BytesN<32>> {
        let key = symbol_short!("all_aud");
        env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    fn matches_filter(entry: &AuditLogEntry, filter: &AuditQueryFilter) -> bool {
        if let Some(invoice_id) = &filter.invoice_id {
            if entry.invoice_id != *invoice_id {
                return false;
            }
        }
        
        if let Some(operation) = &filter.operation {
            if entry.operation != *operation {
                return false;
            }
        }
        
        if let Some(actor) = &filter.actor {
            if entry.actor != *actor {
                return false;
            }
        }
        
        if let Some(start_ts) = filter.start_timestamp {
            if entry.timestamp < start_ts {
                return false;
            }
        }
        
        if let Some(end_ts) = filter.end_timestamp {
            if entry.timestamp > end_ts {
                return false;
            }
        }
        
        true
    }
}

/// Audit trail helper functions
pub fn log_invoice_operation(
    env: &Env,
    invoice_id: BytesN<32>,
    operation: AuditOperation,
    actor: Address,
    old_value: Option<String>,
    new_value: Option<String>,
    amount: Option<i128>,
    additional_data: Option<String>,
) {
    let entry = AuditLogEntry::new(
        env,
        invoice_id,
        operation,
        actor,
        old_value,
        new_value,
        amount,
        additional_data,
    );
    
    AuditStorage::store_audit_entry(env, &entry);
}

/// Log invoice creation
pub fn log_invoice_created(env: &Env, invoice: &Invoice) {
    log_invoice_operation(
        env,
        invoice.id.clone(),
        AuditOperation::InvoiceCreated,
        invoice.business.clone(),
        None,
        Some(format!("Amount: {}, Due: {}", invoice.amount, invoice.due_date)),
        Some(invoice.amount),
        Some(invoice.description.clone()),
    );
}

/// Log invoice status change
pub fn log_invoice_status_change(
    env: &Env,
    invoice_id: BytesN<32>,
    actor: Address,
    old_status: InvoiceStatus,
    new_status: InvoiceStatus,
) {
    let old_value = format!("{:?}", old_status);
    let new_value = format!("{:?}", new_status);
    
    log_invoice_operation(
        env,
        invoice_id,
        AuditOperation::InvoiceStatusChanged,
        actor,
        Some(old_value),
        Some(new_value),
        None,
        None,
    );
}

/// Log invoice funding
pub fn log_invoice_funded(
    env: &Env,
    invoice_id: BytesN<32>,
    investor: Address,
    amount: i128,
) {
    log_invoice_operation(
        env,
        invoice_id,
        AuditOperation::InvoiceFunded,
        investor,
        None,
        Some(format!("Funded with amount: {}", amount)),
        Some(amount),
        None,
    );
}

/// Log payment processing
pub fn log_payment_processed(
    env: &Env,
    invoice_id: BytesN<32>,
    actor: Address,
    amount: i128,
    payment_type: String,
) {
    log_invoice_operation(
        env,
        invoice_id,
        AuditOperation::PaymentProcessed,
        actor,
        None,
        Some(format!("Payment type: {}, Amount: {}", payment_type, amount)),
        Some(amount),
        Some(payment_type),
    );
}