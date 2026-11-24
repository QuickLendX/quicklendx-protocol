//! Core data types for the QuickLendX protocol.
//!
//! This module defines the persistent data structures stored on-chain.
//! All types are annotated with `#[contracttype]` for Soroban compatibility.
//!
//! ## Units convention
//! - **Amounts** (`i128`) are in the smallest currency unit (stroops for XLM: 1 XLM = 10 000 000 stroops).
//! - **Timestamps** (`u64`) are Unix epoch seconds.
//! - **Rates / fees** (`u32`) are in basis points (bps): 1 bps = 0.01 %; 10 000 bps = 100 %.
//!
//! Key design principles:
//! - Direct storage optimisation: minimal nesting for frequently accessed fields.
//! - Future-proofing: optional fields and versioned enums.
//! - Type safety: strong typing for status and categories.
//! - Addresses are used for identity to leverage Soroban's built-in access control.

#![warn(missing_docs)]

use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

/// Lifecycle state of an invoice.
///
/// The state machine is strictly monotonic – a terminal state cannot
/// transition to a non-terminal one. Use [`InvoiceStatus::is_terminal`] to
/// guard against invalid transitions in contract logic.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    /// Invoice has been created but not yet verified by the platform.
    Pending,
    /// Invoice has passed verification and is eligible for funding.
    Verified,
    /// Invoice has been fully funded by an investor bid.
    Funded,
    /// Invoice has been fully paid by the business (terminal).
    Paid,
    /// Invoice payment is overdue and the invoice is in default (terminal).
    Defaulted,
    /// Invoice was cancelled before funding (terminal).
    Cancelled,
    /// Funds were refunded to the investor, e.g. after cancellation (terminal).
    Refunded,
}

impl InvoiceStatus {
    /// Returns `true` when this status is terminal (no further transitions possible).
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

/// Administrative lock state for an invoice.
///
/// A locked invoice cannot be funded, settled or otherwise mutated until an
/// admin explicitly unlocks it. See [`FreezeInfo`] for the accompanying reason
/// and audit metadata.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceLock {
    /// No lock is in effect; normal operations are permitted.
    None,
    /// Invoice is frozen by an admin; most mutations are blocked.
    Frozen,
}

impl InvoiceLock {
    /// Returns `true` when the invoice is currently locked.
    pub fn is_locked(&self) -> bool {
        *self != Self::None
    }
}

/// Lifecycle state of an investor bid.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BidStatus {
    /// Bid has been submitted and is awaiting acceptance.
    Placed,
    /// Bid was accepted and an investment was created from it.
    Accepted,
    /// Investor withdrew the bid before it was accepted.
    Withdrawn,
    /// Bid was not accepted before its `expiration_timestamp` (terminal).
    Expired,
    /// Bid was explicitly cancelled (terminal).
    Cancelled,
}

/// Lifecycle state of an investor position.
///
/// Only [`InvestmentStatus::Active`] is non-terminal; all other variants
/// indicate the investment has reached its final state.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    /// The investment is active, funded, and tracked in the active investment index.
    /// This is the only non-terminal state.
    Active,
    /// Investment funds were withdrawn by the investor (terminal).
    Withdrawn,
    /// Investment completed successfully; the investor has received their payouts (terminal).
    Completed,
    /// The associated invoice defaulted due to non-payment, triggering default/insurance logic (terminal).
    Defaulted,
    /// Investment was refunded due to invoice cancellation (terminal).
    Refunded,
}

/// Lifecycle state of a dispute.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    /// No dispute is active on this invoice.
    None,
    /// A dispute has been filed and is awaiting review.
    Disputed,
    /// The dispute is actively being reviewed by the resolution panel.
    UnderReview,
    /// The dispute has been resolved; see [`DisputeResolution`] for the outcome.
    Resolved,
}

/// Outcome of a resolved dispute.
///
/// Each variant has a stable numeric code (see [`DisputeResolution::code`])
/// that is safe to store in off-chain databases.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeResolution {
    /// No dispute resolution has been recorded yet.
    None,
    /// The dispute was resolved in favour of the business.
    FavorBusiness,
    /// The dispute was resolved in favour of the investor.
    FavorInvestor,
    /// Disputed funds were split between the business and the investor.
    Split,
    /// The dispute was dismissed without a remedy.
    Dismissed,
}

impl DisputeResolution {
    /// Returns the stable wire code for this outcome.
    ///
    /// These codes are stored in off-chain databases and must never be
    /// renumbered after deployment.
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

/// Invoice category used for classification and analytics.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceCategory {
    /// Professional services invoice (consulting, freelance, etc.).
    Services,
    /// Physical goods invoice.
    Goods,
    /// Consulting engagement invoice.
    Consulting,
    /// Logistics and shipping invoice.
    Logistics,
    /// Finished product sales invoice.
    Products,
    /// Manufacturing and production invoice.
    Manufacturing,
    /// Technology or software invoice.
    Technology,
    /// Healthcare or medical services invoice.
    Healthcare,
    /// Catch-all for categories not covered above.
    Other,
}

/// A single line item within an invoice, encoded as a tuple struct.
///
/// Fields (positional):
/// 1. `description` – Human-readable line item description.
/// 2. `quantity`    – Number of units (dimensionless).
/// 3. `unit_price`  – Price per unit in stroops.
/// 4. `total`       – `quantity × unit_price`, pre-computed by the caller (stroops).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineItemRecord(
    /// Human-readable line item description.
    pub String,
    /// Number of units (dimensionless).
    pub u32,
    /// Price per unit in stroops (1 XLM = 10 000 000 stroops).
    pub i128,
    /// Total value: `quantity × unit_price`, in stroops.
    pub i128,
);

/// A single on-chain payment event recorded in [`Invoice::payment_history`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecord {
    /// Amount paid in stroops (1 XLM = 10 000 000 stroops).
    pub amount: i128,
    /// Stellar address of the account that submitted this payment.
    pub payer: Address,
    /// Unix timestamp (seconds) when this payment was recorded.
    pub timestamp: u64,
    /// Off-chain or on-chain transaction reference string.
    pub transaction_id: String,
}

/// Full dispute record attached to an invoice.
///
/// Populated when a dispute is filed; `resolved_*` fields are set only
/// after resolution. Inspect [`DisputeStatus`] on the parent invoice before
/// reading `resolution_outcome`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    /// Address that filed the dispute.
    pub created_by: Address,
    /// Unix timestamp (seconds) when the dispute was filed.
    pub created_at: u64,
    /// Free-text reason provided by the filer.
    pub reason: String,
    /// Supporting evidence submitted with the dispute.
    pub evidence: String,
    /// Free-text resolution notes recorded by the resolver.
    pub resolution: String,
    /// Address of the admin or arbitrator who resolved the dispute.
    pub resolved_by: Address,
    /// Unix timestamp (seconds) when the dispute was resolved.
    pub resolved_at: u64,
    /// Structured outcome of the resolution.
    pub resolution_outcome: DisputeResolution,
}

/// A single star-rating entry for an invoice.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceRating {
    /// Address of the account that submitted this rating.
    pub rater: Address,
    /// Rating score, 1 (lowest) to 5 (highest).
    pub score: u32,
    /// Optional free-text comment accompanying the rating.
    pub comment: String,
    /// Unix timestamp (seconds) when this rating was submitted.
    pub timestamp: u64,
}

/// Reason an admin applied a freeze to a business or invoice.
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
    /// Alias for [`BusinessFreezeReason::AdminAction`] – generic administrative freeze.
    Administrative,
    /// Business KYC was rejected or revoked.
    KYCRejected,
    /// Legal or compliance policy violation.
    ComplianceViolation,
    /// Fraud or suspicious activity detected (alias for [`BusinessFreezeReason::FraudSuspected`]).
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

/// Freeze record stored alongside a frozen invoice.
///
/// Written atomically with the freeze operation so the reason is always
/// available for audit and appeals processing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreezeInfo {
    /// Reason the freeze was applied.
    pub reason: BusinessFreezeReason,
    /// Address of the admin who applied the freeze.
    pub frozen_by: Address,
    /// Unix timestamp (seconds) when the freeze was applied.
    pub frozen_at: u64,
}

/// Reason an admin applied a freeze to an investor account.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvestorFreezeReason {
    /// Generic administrative freeze (admin's discretion).
    AdminAction,
    /// Investor KYC has expired and must be renewed.
    KYCExpired,
    /// Legal or compliance policy violation.
    ComplianceViolation,
    /// Fraud or suspicious activity detected.
    SuspiciousActivity,
    /// Court order or legal hold applied.
    LegalHold,
}

/// Freeze record stored for a frozen investor account.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvestorFreezeInfo {
    /// Reason the freeze was applied.
    pub reason: InvestorFreezeReason,
    /// Address of the admin who applied the freeze.
    pub frozen_by: Address,
    /// Unix timestamp (seconds) when the freeze was applied.
    pub frozen_at: u64,
}

/// Core on-chain invoice record.
///
/// Stored in persistent contract storage keyed by [`Invoice::id`].
/// Large metadata fields (customer name, tax ID, notes) are stored
/// inline to avoid extra read operations on hot paths.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    /// Unique 32-byte invoice identifier (UUID-style, set by the contract at creation).
    pub id: BytesN<32>,
    /// Stellar address of the business that issued this invoice.
    pub business: Address,
    /// Invoice face value in stroops (1 XLM = 10 000 000 stroops). Must be > 0.
    pub amount: i128,
    /// Token contract address for the invoice denomination.
    pub currency: Address,
    /// Unix timestamp (seconds) by which the invoice must be settled.
    pub due_date: u64,
    /// Current lifecycle state of the invoice.
    pub status: InvoiceStatus,
    /// Unix timestamp (seconds) when the invoice was created on-chain.
    pub created_at: u64,
    /// Human-readable invoice description. Max length: `MAX_DESCRIPTION_LENGTH`.
    pub description: String,
    /// Optional customer display name. Max length enforced: `MAX_NAME_LENGTH`.
    pub metadata_customer_name: Option<String>,
    /// Optional customer postal/physical address. Max length enforced: `MAX_ADDRESS_LENGTH`.
    pub metadata_customer_address: Option<String>,
    /// Optional tax identifier (VAT number, EIN, etc.). Max length enforced: `MAX_TAX_ID_LENGTH`.
    pub metadata_tax_id: Option<String>,
    /// Optional free-text notes. Max length enforced: `MAX_NOTES_LENGTH`.
    pub metadata_notes: Option<String>,
    /// Ordered list of line items that make up this invoice.
    pub metadata_line_items: Vec<LineItemRecord>,
    /// Invoice category for analytics and search.
    pub category: InvoiceCategory,
    /// Searchable tags. Max `MAX_INVOICE_TAGS` entries, each max 50 bytes.
    pub tags: Vec<String>,
    /// Net amount placed in escrow after deducting any origination fee (stroops).
    pub funded_amount: i128,
    /// Unix timestamp (seconds) when the invoice was funded, if applicable.
    pub funded_at: Option<u64>,
    /// Address of the investor who funded this invoice, if applicable.
    pub investor: Option<Address>,
    /// Unix timestamp (seconds) when the invoice was settled, if applicable.
    pub settled_at: Option<u64>,
    /// Rolling average of all submitted ratings (1–5 scale). `None` until at least one rating exists.
    pub average_rating: Option<u32>,
    /// Total number of ratings submitted for this invoice.
    pub total_ratings: u32,
    /// Full list of individual rating records.
    pub ratings: Vec<InvoiceRating>,
    /// Current dispute state.
    pub dispute_status: DisputeStatus,
    /// Full dispute record (populated when a dispute is filed).
    pub dispute: Dispute,
    /// Cumulative amount paid by the business across all payment events (stroops).
    pub total_paid: i128,
    /// Ordered history of on-chain payment events.
    pub payment_history: Vec<PaymentRecord>,
    /// Origination fee in basis points (bps), deducted from the gross bid amount.
    /// Capped at `init::MAX_FEE_BPS` (10 %, i.e. 1 000 bps).
    /// Returned on escrow refund; distributable only after escrow release or default.
    pub origination_fee_bps: Option<u32>,
    /// Per-invoice late payment penalty in basis points (0–5 000 bps, i.e. 0–50 %).
    /// Overrides `LATE_FEE_SURCHARGE_BPS` when set. `None` falls back to the 20 % default.
    pub late_payment_penalty_bps: Option<u32>,
    /// Per-invoice early-payment discount in basis points (0–5 000 bps, i.e. 0–50 %).
    /// Applied when payment settles on or before `due_date`. `None` means no discount.
    pub early_payment_discount_bps: Option<u32>,
}

/// Schema version for [`RatingsSnapshot`] serialisation.
///
/// Bump this constant on every breaking change to the snapshot layout.
pub const RATINGS_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Versioned ratings snapshot for off-chain indexers and downstream contracts.
///
/// Emitted alongside rating-change events so consumers can keep a consistent
/// read-model without replaying every individual rating event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RatingsSnapshot {
    /// Schema version; always equal to [`RATINGS_SNAPSHOT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Invoice this snapshot belongs to.
    pub invoice_id: BytesN<32>,
    /// Rolling average rating (1–5), or `None` if no ratings exist yet.
    pub average_rating: Option<u32>,
    /// Total number of ratings included in this snapshot.
    pub total_ratings: u32,
    /// Highest individual rating recorded, or `None` if no ratings exist yet.
    pub highest_rating: Option<u32>,
    /// Lowest individual rating recorded, or `None` if no ratings exist yet.
    pub lowest_rating: Option<u32>,
    /// Ledger sequence number at which this snapshot was produced.
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
    /// Invoice face value in stroops (must be > 0).
    pub amount: i128,
    /// Token contract address for the invoice currency.
    pub currency: Address,
    /// Unix timestamp (seconds) by which the invoice must be settled (must be in the future).
    pub due_date: u64,
    /// Human-readable invoice description (max `MAX_DESCRIPTION_LENGTH` bytes).
    pub description: String,
    /// Invoice category (Services, Products, etc.).
    pub category: InvoiceCategory,
    /// Optional searchable tags (max `MAX_INVOICE_TAGS` entries, each max 50 bytes).
    pub tags: Vec<String>,
    /// Per-invoice late payment penalty in basis points (0–5 000 bps).
    /// Applied when a payment lands after the invoice due date.
    pub late_payment_penalty_bps: Option<u32>,
    /// Per-invoice early-payment discount in basis points (0–5 000 bps).
    /// Applied to a payment that settles on or before `due_date`.
    /// `None` means no discount advertised on this invoice.
    pub early_payment_discount_bps: Option<u32>,
}

/// Metadata update payload for an existing invoice.
///
/// Passed to metadata-update entry points. All fields are validated
/// before being written to the invoice record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceMetadata {
    /// Customer display name. Trimmed and validated to be non-empty. Max: `MAX_NAME_LENGTH`.
    pub customer_name: String,
    /// Customer postal/physical address. Max: `MAX_ADDRESS_LENGTH`.
    pub customer_address: String,
    /// Tax identifier (VAT number, EIN, etc.). Max: `MAX_TAX_ID_LENGTH`.
    pub tax_id: String,
    /// Line items vector. Each item's description validated against `MAX_DESCRIPTION_LENGTH`.
    /// Vector length bounded by `MAX_METADATA_LINE_ITEMS`.
    pub line_items: Vec<LineItemRecord>,
    /// Free-form notes. Max: `MAX_NOTES_LENGTH`.
    pub notes: String,
}

/// Investor bid on an invoice.
///
/// Investors submit bids specifying the amount they are willing to fund and
/// their expected return. The winning bid is accepted by the business and
/// converted into an [`Investment`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bid {
    /// Unique 32-byte bid identifier.
    pub bid_id: BytesN<32>,
    /// Invoice this bid is placed on.
    pub invoice_id: BytesN<32>,
    /// Stellar address of the bidding investor.
    pub investor: Address,
    /// Gross amount supplied by the investor before any origination fee is deducted (stroops).
    pub bid_amount: i128,
    /// Investor's expected return on this bid (stroops).
    pub expected_return: i128,
    /// Unix timestamp (seconds) when the bid was submitted.
    pub timestamp: u64,
    /// Current lifecycle state of the bid.
    pub status: BidStatus,
    /// Unix timestamp (seconds) after which this bid automatically expires if not accepted.
    pub expiration_timestamp: u64,
}

/// Active investor position resulting from an accepted bid.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Investment {
    /// Unique 32-byte investment identifier.
    pub investment_id: BytesN<32>,
    /// Invoice this investment is associated with.
    pub invoice_id: BytesN<32>,
    /// Stellar address of the investor.
    pub investor: Address,
    /// Net principal actually held in escrow and funded to the business (stroops).
    pub amount: i128,
    /// Unix timestamp (seconds) when this investment was funded.
    pub funded_at: u64,
    /// Current lifecycle state of the investment.
    pub status: InvestmentStatus,
    /// Insurance coverage records attached to this investment.
    pub insurance: Vec<InsuranceCoverage>,
}

/// A single insurance coverage record attached to an [`Investment`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceCoverage {
    /// Stellar address of the insurance provider contract.
    pub provider: Address,
    /// Percentage of the investment covered (0–100).
    pub coverage_percentage: u32,
    /// Covered amount in stroops.
    pub coverage_amount: i128,
    /// Premium charged for this coverage in stroops.
    pub premium_amount: i128,
    /// `true` when this coverage is currently active.
    pub active: bool,
}

/// Platform-wide fee configuration set by the admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeConfig {
    /// Platform fee in basis points (1 bps = 0.01 %). Capped at `MAX_FEE_BPS`.
    pub fee_bps: u32,
    /// Treasury address that receives collected fees. `None` disables fee collection.
    pub treasury_address: Option<Address>,
    /// Unix timestamp (seconds) when this configuration was last updated.
    pub updated_at: u64,
    /// Stellar address of the admin who last updated this configuration.
    pub updated_by: Address,
}

/// Relevance rank for invoice search results.
///
/// Higher variants sort earlier when results are ordered by relevance.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum SearchRank {
    /// No specific match; general result included for completeness.
    Other,
    /// Partial text or tag match against the query.
    PartialMatch,
    /// Exact invoice ID match.
    ExactId,
}

/// A single invoice search result entry.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchResult {
    /// Invoice ID of this result.
    pub invoice_id: BytesN<32>,
    /// Relevance rank for ordering results.
    pub rank: SearchRank,
    /// Unix timestamp (seconds) when the invoice was created (used as a tiebreaker).
    pub created_at: u64,
}

/// Report returned by paginated admin index-cleanup helpers.
///
/// The rebuild is paginated and resumable. Pass `next_offset` as the `offset`
/// argument on the next call. The last page is reached when `next_offset`
/// stops advancing.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IndexCleanupReport {
    /// Number of index entries examined in this page.
    pub scanned: u32,
    /// Number of stale entries removed from the index.
    pub removed: u32,
    /// Offset to pass on the next call to continue the cleanup.
    pub next_offset: u32,
}

/// Report returned by paginated admin index-rebuild helpers.
///
/// The rebuild is paginated and resumable. Each helper documents its
/// completion signal and what it counts as `reindexed`.
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
    /// Number of invoices actually pruned (deleted from storage).
    pub pruned: u32,
    /// Offset to pass on the next call.
    pub next_offset: u32,
}

/// A paginated vector of 32-byte identifiers.
///
/// Used by generic list endpoints that return sets of invoice or other IDs.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginatedBytes32Vec {
    /// Page of 32-byte identifiers.
    pub items: Vec<BytesN<32>>,
    /// Total number of items across all pages.
    pub total_count: u32,
    /// `true` when additional pages are available beyond this one.
    pub has_more: bool,
}

/// A paginated page of invoice IDs with generation tracking.
///
/// The `generation` field increments when the underlying index is rebuilt,
/// allowing clients to detect and discard stale cached pages.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorBytes32Page {
    pub items: Vec<BytesN<32>>,
    pub total_count: u32,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorAddressPage {
    pub items: Vec<Address>,
    pub total_count: u32,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CursorBidsPage {
    pub items: Vec<Bid>,
    pub total_count: u32,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoicePage {
    /// Page of invoice IDs.
    pub items: Vec<BytesN<32>>,
    /// Total number of invoices across all pages.
    pub total_count: u32,
    /// `true` when additional pages are available beyond this one.
    pub has_more: bool,
    /// Monotonically increasing index generation counter; increments on rebuild.
    pub generation: u64,
    pub next_cursor: Option<String>,
}

/// A paginated page of [`Bid`] records.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginatedBids {
    /// Page of bid records.
    pub items: Vec<Bid>,
    /// Total number of bids across all pages.
    pub total_count: u32,
    /// `true` when additional pages are available beyond this one.
    pub has_more: bool,
}

/// A paginated page of accepted token contract addresses.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaginatedCurrencies {
    /// Page of token contract addresses.
    pub items: Vec<Address>,
    /// Total number of accepted currencies across all pages.
    pub total_count: u32,
    /// `true` when additional pages are available beyond this one.
    pub has_more: bool,
}
