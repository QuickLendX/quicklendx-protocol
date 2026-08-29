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

use crate::DisputeResolution as OtherDisputeResolution;
use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

/// Invoice status enumeration representing the lifecycle of an invoice
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Pending,
    Verified,
    Funded,
    Paid,
    Defaulted,
    Cancelled,
    Refunded,
}

impl InvoiceStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            InvoiceStatus::Paid
                | InvoiceStatus::Defaulted
                | InvoiceStatus::Cancelled
                | InvoiceStatus::Refunded
        )
    }
}

/// Invoice lock state controlled by admin holds.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceLock {
    None,
    Frozen,
}

impl InvoiceLock {
    pub fn is_locked(&self) -> bool {
        *self != Self::None
    }
}

/// Bid status enumeration
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Accepted,
    Withdrawn,
    Expired,
    Cancelled,
}

/// Investment status enumeration tracking the lifecycle of investor positions.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    /// The investment is active, funded, and tracked in the active investment index.
    /// This is the only non-terminal state.
    Active,
    /// Investment funds were withdrawn by the investor (terminal status).
    Withdrawn,
    /// Investment completed successfully, and the investor has received their payouts (terminal status).
    Completed,
    /// The associated invoice defaulted due to non-payment, triggering default/insurance logic (terminal status).
    Defaulted,
    /// Investment was refunded due to invoice cancellation (terminal status).
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

/// Dispute resolution outcome enumeration
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    None,
    FavorBusiness,
    FavorInvestor,
    Split,
    Dismissed,
}

impl DisputeResolution {
    pub fn code(self) -> u32 {
        match self {
            Self::None => 0,
            Self::FavorBusiness => 1,
            Self::FavorInvestor => 2,
            Self::Split => 3,
            Self::Dismissed => 4,
        }
    }
}

/// Invoice category for classification
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceCategory {
    Services,
    Goods,
    Consulting,
    Logistics,
    Products,
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
    pub resolution_outcome: DisputeResolution,
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

/// Freeze reason enumeration representing why a business invoice was frozen.
///
/// Stored alongside the freeze flag to provide an audit trail and enable
/// targeted unfreeze logic. An admin must supply one of these variants when
/// calling `freeze_invoice`; a bare boolean is no longer sufficient.
///
/// When a freeze is applied, an [`crate::events::InvoiceFrozen`] event is
/// emitted that includes a `freeze_appeal_channel` field pointing to
/// `docs/APPEALS.md` so the affected business knows where to file an appeal.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusinessFreezeReason {
    /// Generic administrative freeze (admin's discretion).
    AdminAction,
    /// Alias for generic administrative freeze
    Administrative,
    /// Business KYC was rejected or revoked
    KYCRejected,
    /// Legal or compliance policy violation.
    ComplianceViolation,
    /// Fraud or suspicious activity detected (alias for `FraudSuspected`).
    SuspiciousActivity,
    /// Court order or legal hold applied.
    LegalHold,
    /// Suspected fraudulent invoice submission or business identity.
    FraudSuspected,
    /// Active or resolved dispute requiring the business to be frozen.
    Dispute,
    /// Business requested a voluntary freeze.
    Voluntary,
}

impl BusinessFreezeReason {
    /// Returns a short human-readable label for event logging.
    pub fn label(&self) -> &'static str {
        match self {
            Self::FraudSuspected => "fraud_suspected",
            Self::ComplianceViolation => "compliance_violation",
            Self::Dispute => "dispute",
            Self::Voluntary => "voluntary",
            Self::AdminAction | Self::Administrative => "admin_action",
            Self::KYCRejected => "kyc_rejected",
            Self::SuspiciousActivity => "suspicious_activity",
            Self::LegalHold => "legal_hold",
        }
    }
}

/// Freeze record stored alongside the frozen flag on an invoice
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreezeInfo {
    pub reason: BusinessFreezeReason,
    pub frozen_by: Address,
    pub frozen_at: u64,
}

/// Freeze reason enumeration representing why an investor was frozen.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestorFreezeReason {
    /// Generic administrative freeze (admin's discretion)
    AdminAction,
    /// Investor KYC has expired
    KYCExpired,
    /// Legal or compliance policy violation
    ComplianceViolation,
    /// Fraud or suspicious activity detected
    SuspiciousActivity,
    /// Court order or legal hold applied
    LegalHold,
}

/// Freeze record stored for a frozen investor
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorFreezeInfo {
    pub reason: InvestorFreezeReason,
    pub frozen_by: Address,
    pub frozen_at: u64,
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
    /// Optional customer display name. Max length enforced: `MAX_NAME_LENGTH`.
    pub metadata_customer_name: Option<String>,
    /// Optional customer address. Max length enforced: `MAX_ADDRESS_LENGTH`.
    pub metadata_customer_address: Option<String>,
    /// Optional tax identifier. Max length enforced: `MAX_TAX_ID_LENGTH`.
    pub metadata_tax_id: Option<String>,
    /// Optional free-text notes. Max length enforced: `MAX_NOTES_LENGTH`.
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
    pub origination_fee_bps: Option<u32>,
    /// Per-invoice late payment penalty in basis points (0–5000 bps, i.e. 0–50%).
    /// When `Some`, this overrides the global `LATE_FEE_SURCHARGE_BPS` constant
    /// during late-payment fee calculation. `None` falls back to the default
    /// 20 % surcharge.
    pub late_payment_penalty_bps: Option<u32>,
    /// Per-invoice early-payment discount in basis points (0–5000 bps, i.e. 0–50%).
    /// When `Some`, this discount is applied to the business's outstanding amount
    /// for any payment that settles on or before the invoice `due_date`.
    /// `None` means "no early-payment discount advertised on this invoice",
    /// preserving the prior flat-fee behaviour.
    pub early_payment_discount_bps: Option<u32>,
}

pub const RATINGS_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Versioned ratings snapshot for off-chain indexers and downstream contracts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatingsSnapshot {
    pub schema_version: u32,
    pub invoice_id: BytesN<32>,
    pub average_rating: Option<u32>,
    pub total_ratings: u32,
    pub highest_rating: Option<u32>,
    pub lowest_rating: Option<u32>,
    pub ledger_sequence: u32,
}

pub const RATINGS_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Versioned ratings snapshot for off-chain indexers and downstream contracts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatingsSnapshot {
    pub schema_version: u32,
    pub invoice_id: BytesN<32>,
    pub average_rating: Option<u32>,
    pub total_ratings: u32,
    pub highest_rating: Option<u32>,
    pub lowest_rating: Option<u32>,
    pub ledger_sequence: u32,
}

/// Input type for a single invoice within a `store_invoices_batch` call.
///
/// Bundles every per-invoice field so the batch entrypoint can accept a
/// `Vec<InvoiceInput>` with a single-argument list, keeping the public ABI
/// clean and future-proof (adding optional metadata requires a new struct
/// version rather than a new variadic argument list).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceInput {
    /// Invoice face value in the smallest currency unit (must be > 0).
    pub amount: i128,
    /// Token contract address for the invoice currency.
    pub currency: Address,
    /// Unix timestamp by which the invoice must be settled (must be in the future).
    pub due_date: u64,
    /// Human-readable invoice description (max `MAX_DESCRIPTION_LENGTH` bytes).
    pub description: String,
    /// Invoice category (Services, Products, etc.).
    pub category: InvoiceCategory,
    /// Optional searchable tags (max `MAX_INVOICE_TAGS` entries, each max 50 bytes).
    pub tags: Vec<String>,
    /// Per-invoice late payment penalty in basis points (0–5000).
    /// Applied when a payment lands after the invoice due date.
    pub late_payment_penalty_bps: Option<u32>,
    /// Per-invoice early-payment discount in basis points (0–5000).
    /// Applied to a payment that settles on or before the invoice `due_date`.
    /// `None` means "no discount advertised on this invoice".
    pub early_payment_discount_bps: Option<u32>,
}

/// Helper struct for metadata updates
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceMetadata {
    /// Customer display name. Trimmed and validated to be non-empty. Max: `MAX_NAME_LENGTH`.
    pub customer_name: String,
    /// Customer address. Max: `MAX_ADDRESS_LENGTH`.
    pub customer_address: String,
    /// Tax identifier (free-form). Max: `MAX_TAX_ID_LENGTH`.
    pub tax_id: String,
    /// Line items vector. Each item's description validated against `MAX_DESCRIPTION_LENGTH`.
    /// The vector length is bounded by `MAX_METADATA_LINE_ITEMS` in verification.
    pub line_items: Vec<LineItemRecord>,
    /// Free-form notes. Max: `MAX_NOTES_LENGTH`.
    pub notes: String,
}

// Invoice logic is implemented in crate::invoice module.

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
    pub treasury_address: Option<Address>,
    pub updated_at: u64,
    pub updated_by: Address,
}

/// Search relevance rank for invoice search results
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SearchRank {
    Other,
    PartialMatch,
    ExactId,
}

/// A single search result entry
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    pub invoice_id: BytesN<32>,
    pub rank: SearchRank,
    pub created_at: u64,
}

/// Report returned by paginated admin rebuild helpers.
///
/// The rebuild is paginated and resumable. Each helper documents its completion
/// signal and what it counts as `reindexed`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexCleanupReport {
    pub scanned: u32,
    pub removed: u32,
    pub next_offset: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RebuildReport {
    /// Number of invoice IDs examined in this page.
    pub scanned: u32,
    /// Number of records repaired or rewritten by the rebuild helper.
    pub reindexed: u32,
    /// Offset to pass on the next call.
    pub next_offset: u32,
}

/// Report returned by paginated admin prune helpers.
///
/// The prune is paginated and resumable. Pass `next_offset` as the `offset`
/// on the next call. The last page is reached when `next_offset` stops advancing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PruneReport {
    /// Number of invoice IDs examined in this page.
    pub scanned: u32,
    /// Number of invoices actually pruned (deleted).
    pub pruned: u32,
    /// Offset to pass on the next call.
    pub next_offset: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginatedBytes32Vec {
    pub items: Vec<BytesN<32>>,
    pub total_count: u32,
    pub has_more: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginatedBids {
    pub items: Vec<crate::bid::Bid>,
    pub total_count: u32,
    pub has_more: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginatedCurrencies {
    pub items: Vec<Address>,
    pub total_count: u32,
    pub has_more: bool,
}
