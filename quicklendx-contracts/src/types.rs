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

use crate::invoice::{
    Dispute as InvoiceDispute, DisputeStatus as InvoiceDisputeStatus, Invoice as InvoiceData,
    InvoiceCategory as InvoiceCategoryData, InvoiceMetadata as InvoiceMetadataData,
    InvoiceRating as InvoiceRatingData, InvoiceStatus as InvoiceStatusData,
    LineItemRecord as LineItemRecordData, PaymentRecord as InvoicePaymentRecord,
};
use crate::bid::{Bid as BidData, BidStatus as BidStatusData};
use crate::investment::{
    InsuranceCoverage as InsuranceCoverageData, Investment as InvestmentData,
    InvestmentStatus as InvestmentStatusData,
};

pub use crate::invoice::{
    Dispute, DisputeStatus, Invoice, InvoiceCategory, InvoiceMetadata, InvoiceRating,
    InvoiceStatus, LineItemRecord, PaymentRecord,
};
pub use crate::bid::{Bid, BidStatus};
pub use crate::investment::{InsuranceCoverage, Investment, InvestmentStatus};

/// Platform fee configuration

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFee {
    pub fee_bps: u32,
    pub recipient: Address,
    pub description: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeConfig {
    pub verification_fee: PlatformFee,
    pub settlement_fee: PlatformFee,
    pub bid_fee: PlatformFee,
    pub investment_fee: PlatformFee,
}

