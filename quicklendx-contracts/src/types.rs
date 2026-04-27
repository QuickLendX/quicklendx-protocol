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

// Re-export BidStatus from bid module to avoid duplication
pub use crate::bid::BidStatus;

/// Search result ranking for invoice search
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum SearchRank {
    ExactMatch = 3,
    PartialMatch = 2,
    Other = 1,
}

/// Search result containing invoice ID and ranking
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub invoice_id: BytesN<32>,
    pub rank: SearchRank,
    pub created_at: u64,
}

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
    Disputed,
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
    Manufacturing,
    Technology,
    Healthcare,
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
    pub created_at: u64,
    pub description: String,
    pub metadata_customer_name: Option<String>,
    pub metadata_customer_address: Option<String>,
    pub metadata_tax_id: Option<String>,
    pub metadata_notes: Option<String>,
    pub metadata_line_items: Vec<LineItemRecord>,
    pub category: InvoiceCategory,
    pub tags: Vec<String>,
    pub funded_amount: i128,
    pub funded_at: Option<u64>,
    pub investor: Option<Address>,
    pub settled_at: Option<u64>,
    pub average_rating: Option<u32>,
    pub total_ratings: u32,
    pub ratings: Vec<InvoiceRating>,
    pub dispute_status: DisputeStatus,
    pub dispute: Dispute,
    pub total_paid: i128,
    pub payment_history: Vec<PaymentRecord>,
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
