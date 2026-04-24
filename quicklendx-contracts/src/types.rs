//! Core data types for the QuickLendX protocol.
//!
//! This module defines the persistent data structures stored in the blockchain.
//! All types are designed for Soroban compatibility using `#[contracttype]`.
//!
//! Key design principles:
//! - Direct storage optimization: minimal nesting for frequently accessed fields
//! - Future-proofing: use of optional fields and versioned enums
//! - Type safety: strong typing for status and categories
//! - Addresses are used for identity to leverage Soroban's built-in access control

use soroban_sdk::{contracttype, Address, BytesN, Env, IntoVal, String, Vec};

/// Invoice status enumeration representing the lifecycle of an invoice
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Pending,
    Verified,
    Funded,
    Paid,
    Defaulted,
    Cancelled,
    Refunded,
}

/// Bid status enumeration
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Accepted,
    Withdrawn,
    Expired,
}

/// Investment status enumeration
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    Active,
    Withdrawn,
    Completed,
    Defaulted,
    Refunded,
}

/// Dispute status enumeration
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    None,
    UnderReview,
    Resolved,
}

/// Invoice category for classification
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceCategory {
    Services,
    Goods,
    Consulting,
    Logistics,
    Other,
}

/// Line item record for invoice metadata
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineItemRecord(pub String, pub u32, pub i128, pub i128);

/// Payment record for invoice history
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecord {
    pub amount: i128,
    pub payer: Address,
    pub timestamp: u64,
    pub transaction_id: String,
}

/// Dispute data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub created_by: Address,
    pub created_at: u64,
    pub reason: String,
    pub evidence: String,
    pub resolution: String,
    pub resolved_by: Address,
    pub resolved_at: u64,
}

/// Invoice rating structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceRating {
    pub rater: Address,
    pub score: u32, // 1-5
    pub comment: String,
    pub timestamp: u64,
}

/// Core Invoice data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub status: InvoiceStatus,
    pub description: String,
    pub category: InvoiceCategory,
    pub tags: Vec<String>,
    
    // Metadata fields
    pub metadata_customer_name: Option<String>,
    pub metadata_customer_address: Option<String>,
    pub metadata_tax_id: Option<String>,
    pub metadata_notes: Option<String>,
    pub metadata_line_items: Vec<LineItemRecord>,
    
    // Financial and lifecycle fields
    pub funded_amount: i128,
    pub funded_at: Option<u64>,
    pub investor: Option<Address>,
    pub settled_at: Option<u64>,
    pub total_paid: i128,
    pub payment_history: Vec<PaymentRecord>,
    
    // Rating fields
    pub average_rating: Option<u32>,
    pub total_ratings: u32,
    pub ratings: Vec<InvoiceRating>,
    
    // Dispute fields
    pub dispute_status: DisputeStatus,
    pub dispute: Dispute,
    
    pub created_at: u64,
    pub updated_at: u64,
}

/// Helper struct for metadata updates
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceMetadata {
    pub customer_name: String,
    pub customer_address: String,
    pub tax_id: String,
    pub line_items: Vec<LineItemRecord>,
    pub notes: String,
}

impl Invoice {
    pub fn new(
        env: &Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: InvoiceCategory,
        tags: Vec<String>,
    ) -> Result<Self, crate::errors::QuickLendXError> {
        let id = env.crypto().sha256(&(&business, &amount, &due_date, env.ledger().timestamp()).into_val(env));
        let timestamp = env.ledger().timestamp();

        Ok(Self {
            id,
            business: business.clone(),
            amount,
            currency,
            due_date,
            status: InvoiceStatus::Pending,
            description,
            category,
            tags,
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(env),
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            total_paid: 0,
            payment_history: Vec::new(env),
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(env),
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: business.clone(),
                created_at: 0,
                reason: String::from_str(env, ""),
                evidence: String::from_str(env, ""),
                resolution: String::from_str(env, ""),
                resolved_by: business.clone(),
                resolved_at: 0,
            },
            created_at: timestamp,
            updated_at: timestamp,
        })
    }

    pub fn verify(&mut self, env: &Env, _actor: Address) {
        self.status = InvoiceStatus::Verified;
        self.updated_at = env.ledger().timestamp();
    }

    pub fn cancel(&mut self, env: &Env, _actor: Address) -> Result<(), crate::errors::QuickLendXError> {
        if self.status != InvoiceStatus::Pending && self.status != InvoiceStatus::Verified {
            return Err(crate::errors::QuickLendXError::InvalidStatus);
        }
        self.status = InvoiceStatus::Cancelled;
        self.updated_at = env.ledger().timestamp();
        Ok(())
    }

    pub fn mark_as_paid(&mut self, env: &Env, _actor: Address, timestamp: u64) {
        self.status = InvoiceStatus::Paid;
        self.settled_at = Some(timestamp);
        self.updated_at = timestamp;
    }

    pub fn mark_as_defaulted(&mut self) {
        self.status = InvoiceStatus::Defaulted;
    }

    pub fn mark_as_funded(&mut self, env: &Env, _actor: Address, amount: i128, timestamp: u64) {
        self.status = InvoiceStatus::Funded;
        self.funded_amount = amount;
        self.funded_at = Some(timestamp);
        self.updated_at = timestamp;
    }

    pub fn set_metadata(&mut self, env: &Env, metadata: Option<InvoiceMetadata>) -> Result<(), crate::errors::QuickLendXError> {
        if let Some(m) = metadata {
            self.metadata_customer_name = Some(m.customer_name);
            self.metadata_customer_address = Some(m.customer_address);
            self.metadata_tax_id = Some(m.tax_id);
            self.metadata_line_items = m.line_items;
            self.metadata_notes = Some(m.notes);
        } else {
            self.metadata_customer_name = None;
            self.metadata_customer_address = None;
            self.metadata_tax_id = None;
            self.metadata_line_items = Vec::new(env);
            self.metadata_notes = None;
        }
        self.updated_at = env.ledger().timestamp();
        Ok(())
    }

    pub fn metadata(&self) -> Option<InvoiceMetadata> {
        if let Some(name) = self.metadata_customer_name.clone() {
            Some(InvoiceMetadata {
                customer_name: name,
                customer_address: self.metadata_customer_address.clone().unwrap_or(String::from_str(self.metadata_line_items.env(), "")),
                tax_id: self.metadata_tax_id.clone().unwrap_or(String::from_str(self.metadata_line_items.env(), "")),
                line_items: self.metadata_line_items.clone(),
                notes: self.metadata_notes.clone().unwrap_or(String::from_str(self.metadata_line_items.env(), "")),
            })
        } else {
            None
        }
    }
}

/// Bid data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bid {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub status: BidStatus,
    pub expiration_timestamp: u64,
}

/// Investment data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub funded_at: u64,
    pub status: InvestmentStatus,
    pub insurance: Vec<InsuranceCoverage>,
}

/// Insurance coverage record
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceCoverage {
    pub provider: Address,
    pub coverage_percentage: u32,
    pub coverage_amount: i128,
    pub premium_amount: i128,
    pub active: bool,
}

/// Platform fee configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeConfig {
    pub fee_bps: u32,
    pub updated_at: u64,
    pub updated_by: Address,
}

/// Platform fee record
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFee {
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub recipient: Address,
    pub timestamp: u64,
}
