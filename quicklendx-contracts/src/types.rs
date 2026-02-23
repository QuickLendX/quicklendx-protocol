//! Core data types for the QuickLendX invoice factoring protocol.
//!
//! This module defines all the fundamental types used throughout the contract,
//! including invoices, bids, investments, and their associated enums and structs.
//!
//! # Security Notes
//!
//! - All types use `#[contracttype]` to ensure proper serialization for on-chain storage
//! - Types are designed to be immutable where possible to prevent unauthorized modifications
//! - Addresses are used for identity to leverage Soroban's built-in access control

use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

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
}

/// Bid status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Withdrawn,
    Accepted,
    Expired,
}

/// Investment status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    Active,
    Withdrawn,
    Completed,
    Defaulted,
}

/// Dispute status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    None,
    Disputed,
    UnderReview,
    Resolved,
}

/// Invoice category enumeration for classification
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceCategory {
    Services,
    Products,
    Consulting,
    Manufacturing,
    Technology,
    Healthcare,
    Other,
}

/// Compact representation of a line item stored on-chain
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineItemRecord(pub String, pub i128, pub i128, pub i128);

/// Metadata associated with an invoice
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvoiceMetadata {
    pub customer_name: String,
    pub customer_address: String,
    pub tax_id: String,
    pub line_items: Vec<LineItemRecord>,
    pub notes: String,
}

/// Individual payment record for an invoice
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecord {
    pub amount: i128,
    pub timestamp: u64,
    pub transaction_id: String,
}

/// Dispute structure
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
#[derive(Clone, Debug)]
pub struct InvoiceRating {
    pub rating: u32,
    pub feedback: String,
    pub rated_by: Address,
    pub rated_at: u64,
}

/// Core invoice data structure
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
    pub metadata: InvoiceMetadata,
    pub dispute: Dispute,
    pub payments: Vec<PaymentRecord>,
    pub ratings: Vec<InvoiceRating>,
    pub created_at: u64,
    pub updated_at: u64,
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

/// Insurance coverage structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceCoverage {
    pub provider: Address,
    pub coverage_amount: i128,
    pub premium_amount: i128,
    pub coverage_percentage: u32,
    pub active: bool,
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

/// Platform fee configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFee {
    pub fee_bps: u32,
    pub recipient: Address,
    pub description: String,
}

/// Platform fee configuration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeConfig {
    pub verification_fee: PlatformFee,
    pub settlement_fee: PlatformFee,
    pub bid_fee: PlatformFee,
    pub investment_fee: PlatformFee,
}
