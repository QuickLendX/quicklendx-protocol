#![allow(deprecated)]

use crate::audit::OpType;
use crate::fees::FeeType;
use crate::payments::Escrow;
use crate::types::Bid;
use crate::types::{Invoice, InvoiceMetadata, PlatformFeeConfig};
use crate::verification::InvestorVerification;
use soroban_sdk::{contractevent, symbol_short, Address, BytesN, Env, String, Symbol, Vec};

// ============================================================================
// Topic Constants
//
// These compile-time constants pin the exact Symbol used as the first topic
// for every event. Off-chain indexers import these to avoid hard-coding
// string literals. Any rename here is a breaking schema change.
// ============================================================================

/// Topic for `InvoiceUploaded` / `InvoiceCreated` events.
/// The `#[contractevent]` macro uses the snake_case struct name as the topic.
pub const TOPIC_INVOICE_UPLOADED: &str = "invoice_uploaded";
/// Topic for `InvoiceVerified` events.
pub const TOPIC_INVOICE_VERIFIED: &str = "invoice_verified";
/// Topic for `InvoiceCancelled` events.
pub const TOPIC_INVOICE_CANCELLED: &str = "invoice_cancelled";
/// Topic for `InvoiceSettled` / `LoanSettled` events.
pub const TOPIC_INVOICE_SETTLED: &str = "invoice_settled";
/// Topic for `InvoiceDefaulted` events.
pub const TOPIC_INVOICE_DEFAULTED: &str = "invoice_defaulted";
/// Topic for `InvoiceExpired` events.
pub const TOPIC_INVOICE_EXPIRED: &str = "invoice_expired";
/// Topic for `PartialPayment` events.
pub const TOPIC_PARTIAL_PAYMENT: &str = "partial_payment";
/// Topic for `PaymentRecorded` events.
pub const TOPIC_PAYMENT_RECORDED: &str = "payment_recorded";
/// Topic for `InvoiceSettledFinal` events.
pub const TOPIC_INVOICE_SETTLED_FINAL: &str = "invoice_settled_final";
/// Topic for `InvoiceFunded` events.
pub const TOPIC_INVOICE_FUNDED: &str = "invoice_funded";
/// Topic for `BidPlaced` events.
pub const TOPIC_BID_PLACED: &str = "bid_placed";
/// Topic for `BidAccepted` events.
pub const TOPIC_BID_ACCEPTED: &str = "bid_accepted";
/// Topic for `BidWithdrawn` events.
pub const TOPIC_BID_WITHDRAWN: &str = "bid_withdrawn";
/// Topic for `BidCancelled` events.
pub const TOPIC_BID_CANCELLED: &str = "bid_cancelled";
/// Topic for `BidExpired` events.
pub const TOPIC_BID_EXPIRED: &str = "bid_expired";
/// Topic for `EscrowCreated` / `FundsLocked` events.
pub const TOPIC_ESCROW_CREATED: &str = "escrow_created";
/// Topic for `EscrowReleased` events.
pub const TOPIC_ESCROW_RELEASED: &str = "escrow_released";
/// Topic for `EscrowRefunded` events.
pub const TOPIC_ESCROW_REFUNDED: &str = "escrow_refunded";
/// Topic for `InvestmentWithdrawn` events.
pub const TOPIC_INVESTMENT_WITHDRAWN: &str = "investment_withdrawn";
/// Topic for `DisputeCreated` / `DisputeOpened` events.
pub const TOPIC_DISPUTE_CREATED: &str = "dispute_created";
/// Topic for `DisputeUnderReview` events.
pub const TOPIC_DISPUTE_UNDER_REVIEW: &str = "dispute_under_review";
/// Topic for `DisputeResolved` events.
pub const TOPIC_DISPUTE_RESOLVED: &str = "dispute_resolved";
/// Topic for `DisputeRejected` events.
pub const TOPIC_DISPUTE_REJECTED: &str = "dispute_rejected";
/// Topic for `TreasuryRotationInitiated` events.
pub const TOPIC_TREASURY_ROTATION_INITIATED: &str = "treasury_rotation_initiated";
/// Topic for `TreasuryRotationConfirmed` events.
pub const TOPIC_TREASURY_ROTATION_CONFIRMED: &str = "treasury_rotation_confirmed";
/// Topic for `TreasuryRotationCancelled` events.
pub const TOPIC_TREASURY_ROTATION_CANCELLED: &str = "treasury_rotation_cancelled";
/// Topic for `InvoiceFrozen` events.
///
/// Emitted when an admin applies a freeze to an invoice via `freeze_invoice`.
/// The payload includes a `freeze_appeal_channel` field that points consumers
/// to the appeals process documented in `docs/APPEALS.md`.
pub const TOPIC_INVOICE_FROZEN: &str = "invoice_frozen";

// ============================================================================
// Storage-schema version and migration topic constants
//
// These pin the exact event topics used by the migration lifecycle machinery.
// Off-chain reconciliation tools must subscribe to these topics to track
// schema upgrades and ensure no committed protocol action is lost.
// Any rename here is a BREAKING schema change.
// ============================================================================

/// Topic emitted when a storage schema migration is started.
///
/// Subscribers can use `schema_from` and `schema_to` to determine which
/// migration is in progress and whether a rollback is needed.
pub const TOPIC_MIGRATION_STARTED: &str = "migration_started";

/// Topic emitted when a storage schema migration completes successfully.
///
/// The `records_migrated` field allows off-chain tools to verify record counts
/// against their own state.
pub const TOPIC_MIGRATION_COMPLETED: &str = "migration_completed";

/// Topic emitted when a storage schema migration is rolled back.
///
/// A rollback leaves storage at `schema_from` with no partial state.
pub const TOPIC_MIGRATION_ROLLED_BACK: &str = "migration_rolled_back";

/// Topic emitted when a storage schema migration fails partway through.
///
/// The `records_migrated` field indicates how many records were processed
/// before the failure.  The migration is resumable from `next_offset`.
pub const TOPIC_MIGRATION_FAILED: &str = "migration_failed";

/// Topic emitted when the storage schema version is recorded or updated.
pub const TOPIC_SCHEMA_VERSION_SET: &str = "schema_version_set";

/// Topic constants for upgrade lifecycle events.
pub const TOPIC_UPGRADE_SCHEDULED: &str = "upg_sch";
pub const TOPIC_UPGRADE_CANCELLED: &str = "upg_can";
pub const TOPIC_UPGRADE_EXECUTED: &str = "upg_exe";

// ============================================================================
// Protocol-level semantic aliases
//
// The task specification uses domain-level names. These type aliases map them
// to the canonical event types so both names compile and refer to the same
// schema. Off-chain indexers should subscribe to the TOPIC_* constants above.
// ============================================================================

/// Semantic alias: `InvoiceCreated` == `InvoiceUploaded`.
/// Both refer to the same event schema; use `TOPIC_INVOICE_UPLOADED` as the topic.
pub type InvoiceCreated = InvoiceUploaded;

/// Semantic alias: `FundsLocked` == `EscrowCreated`.
/// Emitted when investor funds are locked in escrow upon bid acceptance.
/// Use `TOPIC_ESCROW_CREATED` as the topic.
pub type FundsLocked = EscrowCreated;

/// Semantic alias: `LoanSettled` == `InvoiceSettled`.
/// Emitted when a loan (invoice) is fully settled.
/// Use `TOPIC_INVOICE_SETTLED` as the topic.
pub type LoanSettled = InvoiceSettled;

/// Semantic alias: `DisputeOpened` == `DisputeCreated`.
/// Use `TOPIC_DISPUTE_CREATED` as the topic.
pub type DisputeOpened = DisputeCreated;

// ============================================================================
// Structured Event Types
// ============================================================================

/// Emitted when a new invoice is uploaded / created by a business.
///
/// Topic: [`TOPIC_INVOICE_UPLOADED`] (`"inv_up"`)
///
/// # Fields
/// - `invoice_id` – Unique 32-byte invoice identifier.
/// - `business` – Address of the business that owns the invoice.
/// - `amount` – Invoice face value in the smallest currency unit.
/// - `currency` – Token contract address for the invoice currency.
/// - `due_date` – Unix timestamp when the invoice is due.
/// - `timestamp` – Ledger timestamp at emission time.
#[contractevent]
pub struct InvoiceUploaded {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub timestamp: u64,
}

/// Emitted when an invoice is verified by an admin.
///
/// Topic: [`TOPIC_INVOICE_VERIFIED`] (`"inv_ver"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceVerified {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}

/// Emitted when an invoice is cancelled by the business owner.
///
/// Topic: [`TOPIC_INVOICE_CANCELLED`] (`"inv_canc"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceCancelled {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub timestamp: u64,
}

/// Emitted when an invoice is fully settled (loan repaid).
///
/// Topic: [`TOPIC_INVOICE_SETTLED`] (`"inv_set"`)
///
/// # Fields
/// - `invoice_id` – Unique 32-byte invoice identifier.
/// - `amount` – Total amount settled (sum of all payments applied to this invoice).
/// - `ledger` – Ledger sequence number at the time of settlement.
/// - `business` – Address of the business that owns the invoice.
/// - `investor` – Address of the investor who funded the invoice.
/// - `investor_return` – Amount returned to the investor after fees.
/// - `platform_fee` – Fee taken by the platform.
/// - `timestamp` – Ledger timestamp at emission time.
///
/// # Security
/// No PII is included. `investor_return` and `platform_fee` are derived
/// from validated contract state only.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceSettled {
    pub invoice_id: BytesN<32>,
    pub amount: i128,
    pub ledger: u32,
    pub business: Address,
    pub investor: Address,
    pub investor_return: i128,
    pub platform_fee: i128,
    pub timestamp: u64,
}

/// Emitted when an invoice is marked as defaulted.
///
/// Topic: [`TOPIC_INVOICE_DEFAULTED`] (`"inv_def"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceDefaulted {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub investor: Address,
    pub timestamp: u64,
}

/// Emitted when an invoice expires past its due date without payment.
///
/// Topic: [`TOPIC_INVOICE_EXPIRED`] (`"inv_exp"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceExpired {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub due_date: u64,
}

/// Emitted on each partial payment towards an invoice.
///
/// Topic: [`TOPIC_PARTIAL_PAYMENT`] (`"inv_pp"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct PartialPayment {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub payment_amount: i128,
    pub total_paid: i128,
    pub progress: u32,
    pub transaction_id: String,
}

/// Emitted when a payment record is durably stored.
///
/// Topic: [`TOPIC_PAYMENT_RECORDED`] (`"pay_rec"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct PaymentRecorded {
    pub invoice_id: BytesN<32>,
    pub payer: Address,
    pub amount: i128,
    pub transaction_id: String,
    pub timestamp: u64,
}

/// Emitted when an invoice reaches final settlement (all funds disbursed).
///
/// Topic: [`TOPIC_INVOICE_SETTLED_FINAL`] (`"inv_stlf"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceSettledFinal {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub investor: Address,
    pub total_paid: i128,
    pub timestamp: u64,
}

/// Emitted when a bid is placed on an invoice.
///
/// Topic: [`TOPIC_BID_PLACED`] (`"bid_plc"`)
///
/// # Fields
/// - `bid_id` – Unique bid identifier.
/// - `invoice_id` – The invoice being bid on (auction_id in protocol terms).
/// - `investor` – Address of the bidder.
/// - `bid_amount` – Amount offered by the investor.
/// - `expected_return` – Total expected repayment amount.
/// - `timestamp` – Ledger timestamp when bid was placed.
/// - `expiration_timestamp` – Timestamp after which the bid expires.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidPlaced {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub expiration_timestamp: u64,
}

/// Emitted when a bid is accepted by the business owner.
///
/// Topic: [`TOPIC_BID_ACCEPTED`] (`"bid_acc"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidAccepted {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
}

/// Emitted when an investor withdraws their bid.
///
/// Topic: [`TOPIC_BID_WITHDRAWN`] (`"bid_wdr"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidWithdrawn {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub timestamp: u64,
}

/// Emitted when a bid is cancelled by its investor.
///
/// Topic: [`TOPIC_BID_CANCELLED`] (`"bid_cancelled"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidCancelled {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub timestamp: u64,
}

/// Emitted when a bid expires past its TTL.
///
/// Topic: [`TOPIC_BID_EXPIRED`] (`"bid_exp"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct BidExpired {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expiration_timestamp: u64,
}

/// Emitted when investor funds are locked in escrow (bid accepted).
///
/// Topic: [`TOPIC_ESCROW_CREATED`] (`"esc_cr"`)
///
/// # Fields
/// - `escrow_id` – Unique escrow identifier.
/// - `invoice_id` – The invoice being funded.
/// - `investor` – Address of the investor whose funds are locked.
/// - `business` – Address of the business receiving the funds.
/// - `amount` – Amount locked in escrow.
///
/// # Security
/// Funds are locked atomically with bid acceptance. No PII included.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct EscrowCreated {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub amount: i128,
}

/// Emitted when escrow funds are released to the business.
///
/// Topic: [`TOPIC_ESCROW_RELEASED`] (`"esc_rel"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct EscrowReleased {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
}

/// Emitted when escrow funds are refunded to the investor.
///
/// Topic: [`TOPIC_ESCROW_REFUNDED`] (`"esc_ref"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct EscrowRefunded {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
}

/// Emitted when an investor withdraws their investment before settlement.
///
/// Topic: [`TOPIC_INVESTMENT_WITHDRAWN`] (`"inv_wd"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvestmentWithdrawn {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
}

/// Emitted when invoice metadata is updated.
///
/// Topic: `"invoice_metadata_updated"`
///
/// # Security
/// **NO PII**: This event does NOT include customer_name or tax_id to prevent
/// PII leakage. Only aggregate statistics (line_item_count, total_value) are included.
#[contractevent]
pub struct InvoiceMetadataUpdated {
    pub invoice_id: BytesN<32>,
    pub line_item_count: u32,
    pub total_value: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct InvoiceMetadataCleared {
    pub invoice_id: BytesN<32>,
    pub business: Address,
}

#[contractevent]
pub struct InvestorVerified {
    pub investor: Address,
    pub investment_limit: i128,
    pub verified_at: u64,
}

#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceFunded {
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct InsuranceAdded {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub provider: Address,
    pub coverage_percentage: u32,
    pub coverage_amount: i128,
    pub premium_amount: i128,
}

#[contractevent]
pub struct InsurancePremiumCollected {
    pub investment_id: BytesN<32>,
    pub provider: Address,
    pub premium_amount: i128,
}

#[contractevent]
pub struct InsuranceClaimed {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub provider: Address,
    pub coverage_amount: i128,
}

#[contractevent]
pub struct PlatformFeeUpdated {
    pub fee_bps: u32,
    pub updated_at: u64,
    pub updated_by: Address,
}

#[contractevent]
pub struct FeeStructureUpdated {
    pub fee_type: FeeType,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct PlatformFeeRouted {
    pub invoice_id: BytesN<32>,
    pub recipient: Address,
    pub fee_amount: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct PlatformFeeConfigUpdated {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct TreasuryConfigured {
    pub treasury_address: Address,
    pub configured_by: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct TreasuryRotationInitiated {
    pub new_address: Address,
    pub initiated_by: Address,
    pub confirmation_deadline: u64,
    pub timestamp: u64,
}

#[contractevent]
pub struct TreasuryRotationConfirmed {
    pub old_address: Address,
    pub new_address: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupCreated {
    pub backup_id: BytesN<32>,
    pub invoice_count: u32,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupRestored {
    pub backup_id: BytesN<32>,
    pub invoice_count: u32,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupValidated {
    pub backup_id: BytesN<32>,
    pub success: bool,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupArchived {
    pub backup_id: BytesN<32>,
    pub timestamp: u64,
}

#[contractevent]
pub struct RetentionPolicyUpdated {
    pub max_backups: u32,
    pub max_age_seconds: u64,
    pub auto_cleanup_enabled: bool,
    pub timestamp: u64,
}

#[contractevent]
pub struct BackupsCleaned {
    pub removed_count: u32,
    pub timestamp: u64,
}

#[contractevent]
pub struct AuditValidation {
    pub invoice_id: BytesN<32>,
    pub is_valid: bool,
    pub timestamp: u64,
}

#[contractevent]
pub struct AuditQuery {
    pub query_type: OpType,
    pub result_count: u32,
}

#[contractevent]
pub struct InvoiceCategoryUpdated {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub old_category: crate::types::InvoiceCategory,
    pub new_category: crate::types::InvoiceCategory,
}

#[contractevent]
pub struct InvoiceTagAdded {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub tag: String,
}

#[contractevent]
pub struct InvoiceTagRemoved {
    pub invoice_id: BytesN<32>,
    pub business: Address,
    pub tag: String,
}

/// Emitted when a dispute is opened on an invoice.
///
/// Topic: [`TOPIC_DISPUTE_CREATED`] (`"dsp_cr"`)
///
/// # Fields
/// - `invoice_id` – The disputed invoice.
/// - `created_by` – Address of the dispute initiator (business or investor).
/// - `reason` – Human-readable reason string (no PII, max 1000 chars).
/// - `timestamp` – Ledger timestamp at emission time.
///
/// # Security
/// Only the business owner or investor on the invoice may open a dispute.
/// The `reason` field must not contain PII; it is a reason code or short description.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeCreated {
    pub invoice_id: BytesN<32>,
    pub created_by: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Emitted when a dispute is moved to admin review.
///
/// Topic: [`TOPIC_DISPUTE_UNDER_REVIEW`] (`"dsp_ur"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeUnderReview {
    pub invoice_id: BytesN<32>,
    pub reviewed_by: Address,
    pub timestamp: u64,
}

/// Emitted when a dispute is resolved by an admin.
///
/// Topic: [`TOPIC_DISPUTE_RESOLVED`] (`"dsp_rs"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeResolved {
    pub invoice_id: BytesN<32>,
    pub resolved_by: Address,
    pub resolution: String,
    pub timestamp: u64,
}

/// Emitted when a dispute is rejected (dismissed) by an admin.
///
/// Topic: [`TOPIC_DISPUTE_REJECTED`] (`"dsp_rj"`)
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct DisputeRejected {
    pub invoice_id: BytesN<32>,
    pub rejected_by: Address,
    pub reason: String,
    pub timestamp: u64,
}

#[contractevent]
pub struct ProfitFeeBreakdown {
    pub invoice_id: BytesN<32>,
    pub investment_amount: i128,
    pub payment_amount: i128,
    pub gross_profit: i128,
    pub platform_fee: i128,
    pub investor_return: i128,
    pub fee_bps_applied: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct TtlExtended {
    pub kind: String,
    pub count: u32,
}

#[contractevent]
pub struct BidTtlUpdated {
    pub old_days: u64,
    pub new_days: u64,
    pub admin: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct BidExpiryGraceUpdated {
    pub old_seconds: u64,
    pub new_seconds: u64,
    pub admin: Address,
    pub timestamp: u64,
}

pub fn emit_ttl_extended(env: &Env, kind: &String, count: u32) {
    TtlExtended {
        kind: kind.clone(),
        count,
    }
    .publish(env);
}

#[contractevent]
pub struct RevenueDistributed {
    pub period: u64,
    pub treasury_amount: i128,
    pub developer_amount: i128,
    pub platform_amount: i128,
}

#[contractevent]
pub struct InvoiceStatusUpdated {
    pub invoice_id: BytesN<32>,
    pub status: crate::types::InvoiceStatus,
}

#[contractevent]
pub struct AdminInitialized {
    pub admin: Address,
}

#[contractevent]
pub struct ProtocolInitialized {
    pub admin: Address,
    pub treasury: Address,
    pub fee_bps: u32,
    pub min_invoice_amount: i128,
    pub max_due_date_days: u64,
    pub grace_period_seconds: u64,
    pub backfill_max_batch_size: u32,
    pub corridors: Vec<Address>,
    pub timestamp: u64,
}

// ============================================================================
// Pause Control Events

#[contractevent]
pub struct Paused {
    pub admin: Address,
}

#[contractevent]
pub struct Unpaused {
    pub admin: Address,
}

pub fn emit_paused(env: &Env, admin: &Address) {
    Paused {
        admin: admin.clone(),
    }
    .publish(env);
}

pub fn emit_unpaused(env: &Env, admin: &Address) {
    Unpaused {
        admin: admin.clone(),
    }
    .publish(env);
}

// ============================================================================
// Freeze / Unfreeze Events
// ============================================================================

/// Emitted when an admin applies a freeze to an invoice via `freeze_invoice`.
///
/// Topic: [`TOPIC_INVOICE_FROZEN`] (`"invoice_frozen"`)
///
/// # Fields
/// - `invoice_id` – The frozen invoice.
/// - `frozen_by` – Address of the admin who applied the freeze.
/// - `reason` – Machine-readable label for the [`crate::types::BusinessFreezeReason`]
///   variant (e.g. `"admin_action"`, `"compliance_violation"`, `"fraud_suspected"`).
/// - `freeze_appeal_channel` – A short pointer to the off-chain appeals process.
///   Set to `"docs/APPEALS.md"` by the emitter.  Downstream consumers (dashboards,
///   notification pipelines) can surface this string directly to the affected
///   business so they know where to file an appeal without paging an engineer.
///   This field contains **no PII** — it is a static URL/path.
/// - `timestamp` – Ledger timestamp at emission time.
///
/// # Backwards compatibility
/// This event is **additive**.  Indexers that do not recognise the
/// `freeze_appeal_channel` field can safely ignore it.
///
/// # Security
/// No PII is included.  The `reason` field is the string label returned by
/// `BusinessFreezeReason::label()`, not a free-text admin comment.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvoiceFrozen {
    pub invoice_id: BytesN<32>,
    pub frozen_by: Address,
    pub reason: String,
    pub freeze_appeal_channel: String,
    pub timestamp: u64,
}

/// Emit an [`InvoiceFrozen`] event.
///
/// `reason_label` should come from [`crate::types::BusinessFreezeReason::label()`].
/// `freeze_appeal_channel` is always `"docs/APPEALS.md"` — a static pointer
/// to the operator-facing appeals runbook so that any off-chain consumer of
/// this event knows immediately where to direct the frozen business.
pub fn emit_invoice_frozen(
    env: &Env,
    invoice_id: &BytesN<32>,
    frozen_by: &Address,
    reason_label: &str,
) {
    InvoiceFrozen {
        invoice_id: invoice_id.clone(),
        frozen_by: frozen_by.clone(),
        reason: String::from_str(env, reason_label),
        freeze_appeal_channel: String::from_str(env, "docs/APPEALS.md"),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Invoice Event Emitters
// ============================================================================

pub fn emit_invoice_uploaded(env: &Env, invoice: &Invoice) {
    InvoiceUploaded {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        amount: invoice.amount,
        currency: invoice.currency.clone(),
        due_date: invoice.due_date,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_verified(env: &Env, invoice: &Invoice) {
    InvoiceVerified {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_cancelled(env: &Env, invoice: &Invoice) {
    InvoiceCancelled {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_metadata_updated(env: &Env, invoice: &Invoice, metadata: &InvoiceMetadata) {
    let mut total = 0i128;
    for record in metadata.line_items.iter() {
        total = total.saturating_add(record.3);
    }

    InvoiceMetadataUpdated {
        invoice_id: invoice.id.clone(),
        line_item_count: metadata.line_items.len(),
        total_value: total,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_metadata_cleared(env: &Env, invoice: &Invoice) {
    InvoiceMetadataCleared {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
    }
    .publish(env);
}

pub fn emit_investor_verified(env: &Env, verification: &InvestorVerification) {
    InvestorVerified {
        investor: verification.investor.clone(),
        investment_limit: verification.investment_limit,
        verified_at: verification.verified_at.unwrap_or(0),
    }
    .publish(env);
}

pub fn emit_invoice_settled(
    env: &Env,
    invoice: &crate::types::Invoice,
    investor_return: i128,
    platform_fee: i128,
) {
    InvoiceSettled {
        invoice_id: invoice.id.clone(),
        amount: invoice.total_paid,
        ledger: env.ledger().sequence(),
        business: invoice.business.clone(),
        investor: invoice.investor.clone().unwrap_or(Address::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        )),
        investor_return,
        platform_fee,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_partial_payment(
    env: &Env,
    invoice: &Invoice,
    payment_amount: i128,
    total_paid: i128,
    progress: u32,
    transaction_id: String,
) {
    PartialPayment {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        payment_amount,
        total_paid,
        progress,
        transaction_id,
    }
    .publish(env);
}

pub fn emit_payment_recorded(
    env: &Env,
    invoice_id: &BytesN<32>,
    payer: &Address,
    amount: i128,
    transaction_id: String,
) {
    PaymentRecorded {
        invoice_id: invoice_id.clone(),
        payer: payer.clone(),
        amount,
        transaction_id,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_settled_final(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    investor: &Address,
    total_paid: i128,
) {
    InvoiceSettledFinal {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        investor: investor.clone(),
        total_paid,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_expired(env: &Env, invoice: &crate::types::Invoice) {
    InvoiceExpired {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        due_date: invoice.due_date,
    }
    .publish(env);
}

pub fn emit_invoice_defaulted(env: &Env, invoice: &crate::types::Invoice) {
    InvoiceDefaulted {
        invoice_id: invoice.id.clone(),
        business: invoice.business.clone(),
        investor: invoice.investor.clone().unwrap_or(Address::from_str(
            env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        )),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_invoice_funded(env: &Env, invoice_id: &BytesN<32>, investor: &Address, amount: i128) {
    InvoiceFunded {
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Insurance Event Emitters
// ============================================================================

pub fn emit_insurance_added(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    provider: &Address,
    coverage_percentage: u32,
    coverage_amount: i128,
    premium_amount: i128,
) {
    InsuranceAdded {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        provider: provider.clone(),
        coverage_percentage,
        coverage_amount,
        premium_amount,
    }
    .publish(env);
}

pub fn emit_insurance_premium_collected(
    env: &Env,
    investment_id: &BytesN<32>,
    provider: &Address,
    premium_amount: i128,
) {
    InsurancePremiumCollected {
        investment_id: investment_id.clone(),
        provider: provider.clone(),
        premium_amount,
    }
    .publish(env);
}

pub fn emit_insurance_claimed(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    provider: &Address,
    coverage_amount: i128,
) {
    InsuranceClaimed {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        provider: provider.clone(),
        coverage_amount,
    }
    .publish(env);
}

// ============================================================================
// Platform Fee Event Emitters
// ============================================================================

pub fn emit_platform_fee_updated(env: &Env, config: &PlatformFeeConfig) {
    PlatformFeeUpdated {
        fee_bps: config.fee_bps,
        updated_at: config.updated_at,
        updated_by: config.updated_by.clone(),
    }
    .publish(env);
}

pub fn emit_fee_structure_updated(
    env: &Env,
    fee_type: &FeeType,
    old_fee_bps: u32,
    new_fee_bps: u32,
    updated_by: &Address,
) {
    FeeStructureUpdated {
        fee_type: fee_type.clone(),
        old_fee_bps,
        new_fee_bps,
        updated_by: updated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_platform_fee_routed(
    env: &Env,
    invoice_id: &BytesN<32>,
    recipient: &Address,
    fee_amount: i128,
) {
    PlatformFeeRouted {
        invoice_id: invoice_id.clone(),
        recipient: recipient.clone(),
        fee_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_platform_fee_config_updated(
    env: &Env,
    old_fee_bps: u32,
    new_fee_bps: u32,
    updated_by: &Address,
) {
    PlatformFeeConfigUpdated {
        old_fee_bps,
        new_fee_bps,
        updated_by: updated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_treasury_configured(env: &Env, treasury_address: &Address, configured_by: &Address) {
    TreasuryConfigured {
        treasury_address: treasury_address.clone(),
        configured_by: configured_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Escrow Event Emitters
// ============================================================================

pub fn emit_escrow_created(env: &Env, escrow: &Escrow) {
    EscrowCreated {
        escrow_id: escrow.escrow_id.clone(),
        invoice_id: escrow.invoice_id.clone(),
        investor: escrow.investor.clone(),
        business: escrow.business.clone(),
        amount: escrow.amount,
    }
    .publish(env);
}

pub fn emit_escrow_released(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    business: &Address,
    amount: i128,
) {
    EscrowReleased {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        amount,
    }
    .publish(env);
}

pub fn emit_escrow_refunded(
    env: &Env,
    escrow_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    EscrowRefunded {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount,
    }
    .publish(env);
}

pub fn emit_investment_withdrawn(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
) {
    InvestmentWithdrawn {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount,
    }
    .publish(env);
}

// ============================================================================
// Bid Event Emitters
// ============================================================================

pub fn emit_bid_placed(env: &Env, bid: &Bid) {
    BidPlaced {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        expected_return: bid.expected_return,
        timestamp: bid.timestamp,
        expiration_timestamp: bid.expiration_timestamp,
    }
    .publish(env);
}

pub fn emit_bid_withdrawn(env: &Env, bid: &Bid) {
    BidWithdrawn {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_cancelled(env: &Env, bid: &Bid) {
    BidCancelled {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_accepted(env: &Env, bid: &Bid, invoice_id: &BytesN<32>, business: &Address) {
    BidAccepted {
        bid_id: bid.bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: bid.investor.clone(),
        business: business.clone(),
        bid_amount: bid.bid_amount,
        expected_return: bid.expected_return,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_expired(env: &Env, bid: &Bid) {
    BidExpired {
        bid_id: bid.bid_id.clone(),
        invoice_id: bid.invoice_id.clone(),
        investor: bid.investor.clone(),
        bid_amount: bid.bid_amount,
        expiration_timestamp: bid.expiration_timestamp,
    }
    .publish(env);
}

// ============================================================================
// Backup Event Emitters
// ============================================================================

pub fn emit_backup_created(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    BackupCreated {
        backup_id: backup_id.clone(),
        invoice_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_restored(env: &Env, backup_id: &BytesN<32>, invoice_count: u32) {
    BackupRestored {
        backup_id: backup_id.clone(),
        invoice_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_validated(env: &Env, backup_id: &BytesN<32>, success: bool) {
    BackupValidated {
        backup_id: backup_id.clone(),
        success,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backup_archived(env: &Env, backup_id: &BytesN<32>) {
    BackupArchived {
        backup_id: backup_id.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_retention_policy_updated(
    env: &Env,
    max_backups: u32,
    max_age_seconds: u64,
    auto_cleanup_enabled: bool,
) {
    RetentionPolicyUpdated {
        max_backups,
        max_age_seconds,
        auto_cleanup_enabled,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_backups_cleaned(env: &Env, removed_count: u32) {
    BackupsCleaned {
        removed_count,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Audit Event Emitters
// ============================================================================

pub fn emit_audit_validation(env: &Env, invoice_id: &BytesN<32>, is_valid: bool) {
    AuditValidation {
        invoice_id: invoice_id.clone(),
        is_valid,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_audit_query(env: &Env, query_type: OpType, result_count: u32) {
    AuditQuery {
        query_type,
        result_count,
    }
    .publish(env);
}

// ============================================================================
// Invoice Category / Tag Event Emitters
// ============================================================================

pub fn emit_invoice_category_updated(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    old_category: &crate::types::InvoiceCategory,
    new_category: &crate::types::InvoiceCategory,
) {
    InvoiceCategoryUpdated {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        old_category: *old_category,
        new_category: *new_category,
    }
    .publish(env);
}

pub fn emit_invoice_tag_added(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    InvoiceTagAdded {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        tag: tag.clone(),
    }
    .publish(env);
}

pub fn emit_invoice_tag_removed(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    tag: &String,
) {
    InvoiceTagRemoved {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        tag: tag.clone(),
    }
    .publish(env);
}

// ============================================================================
// Dispute Event Emitters
// ============================================================================

pub fn emit_dispute_created(
    env: &Env,
    invoice_id: &BytesN<32>,
    created_by: &Address,
    reason: &String,
) {
    DisputeCreated {
        invoice_id: invoice_id.clone(),
        created_by: created_by.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_under_review(env: &Env, invoice_id: &BytesN<32>, reviewed_by: &Address) {
    DisputeUnderReview {
        invoice_id: invoice_id.clone(),
        reviewed_by: reviewed_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_resolved(
    env: &Env,
    invoice_id: &BytesN<32>,
    resolved_by: &Address,
    resolution: &String,
) {
    DisputeResolved {
        invoice_id: invoice_id.clone(),
        resolved_by: resolved_by.clone(),
        resolution: resolution.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_dispute_rejected(
    env: &Env,
    invoice_id: &BytesN<32>,
    rejected_by: &Address,
    reason: &String,
) {
    DisputeRejected {
        invoice_id: invoice_id.clone(),
        rejected_by: rejected_by.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ============================================================================
// Profit / Fee Breakdown Event Emitter
// ============================================================================

#[allow(dead_code)]
pub fn emit_profit_fee_breakdown(
    env: &Env,
    invoice_id: &BytesN<32>,
    investment_amount: i128,
    payment_amount: i128,
    gross_profit: i128,
    platform_fee: i128,
    investor_return: i128,
    fee_bps_applied: i128,
) {
    ProfitFeeBreakdown {
        invoice_id: invoice_id.clone(),
        investment_amount,
        payment_amount,
        gross_profit,
        platform_fee,
        investor_return,
        fee_bps_applied,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_ttl_updated(env: &Env, old_days: u64, new_days: u64, admin: &Address) {
    BidTtlUpdated {
        old_days,
        new_days,
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bid_expiry_grace_updated(
    env: &Env,
    old_seconds: u64,
    new_seconds: u64,
    admin: &Address,
) {
    BidExpiryGraceUpdated {
        old_seconds,
        new_seconds,
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

#[contractevent]
pub struct EmergencyWithdrawalInitiated {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub unlock_at: u64,
    pub admin: Address,
}

#[contractevent]
pub struct EmergencyWithdrawalExecuted {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub admin: Address,
}

#[contractevent]
pub struct EmergencyWithdrawalCancelled {
    pub token: Address,
    pub amount: i128,
    pub target: Address,
    pub admin: Address,
}

#[contractevent]
pub struct AdminSet {
    pub admin: Address,
    pub timestamp: u64,
}

pub fn emit_emergency_withdrawal_initiated(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    unlock_at: u64,
    admin: Address,
) {
    EmergencyWithdrawalInitiated {
        token,
        amount,
        target,
        unlock_at,
        admin,
    }
    .publish(env);
}

pub fn emit_emergency_withdrawal_executed(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    admin: Address,
) {
    EmergencyWithdrawalExecuted {
        token,
        amount,
        target,
        admin,
    }
    .publish(env);
}

pub fn emit_emergency_withdrawal_cancelled(
    env: &Env,
    token: Address,
    amount: i128,
    target: Address,
    admin: Address,
) {
    EmergencyWithdrawalCancelled {
        token,
        amount,
        target,
        admin,
    }
    .publish(env);
}

pub fn emit_admin_set(env: &Env, admin: &Address) {
    AdminSet {
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_admin_transferred(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_trf"),),
        (
            old_admin.clone(),
            new_admin.clone(),
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_admin_transfer_initiated(env: &Env, current_admin: &Address, pending_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_req"),),
        (
            current_admin.clone(),
            pending_admin.clone(),
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_admin_transfer_cancelled(env: &Env, current_admin: &Address, pending_admin: &Address) {
    env.events().publish(
        (symbol_short!("adm_cnl"),),
        (
            current_admin.clone(),
            pending_admin.clone(),
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_admin_two_step_updated(env: &Env, admin: &Address, enabled: bool) {
    env.events().publish(
        (symbol_short!("adm_2st"),),
        (admin.clone(), enabled, env.ledger().timestamp()),
    );
}

pub fn emit_revenue_distributed(
    env: &Env,
    period: u64,
    treasury_amount: i128,
    developer_amount: i128,
    platform_amount: i128,
) {
    RevenueDistributed {
        period,
        treasury_amount,
        developer_amount,
        platform_amount,
    }
    .publish(env);
}

pub fn emit_invoice_status_updated(
    env: &Env,
    invoice_id: BytesN<32>,
    status: crate::types::InvoiceStatus,
) {
    InvoiceStatusUpdated { invoice_id, status }.publish(env);
}

pub fn emit_protocol_initialized(
    env: &Env,
    admin: &Address,
    treasury: &Address,
    fee_bps: u32,
    min_invoice_amount: i128,
    max_due_date_days: u64,
    grace_period_seconds: u64,
    backfill_max_batch_size: u32,
    corridors: &Vec<Address>,
) {
    ProtocolInitialized {
        admin: admin.clone(),
        treasury: treasury.clone(),
        fee_bps,
        min_invoice_amount,
        max_due_date_days,
        grace_period_seconds,
        backfill_max_batch_size,
        corridors: corridors.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_admin_initialized(env: &Env, admin: &Address) {
    env.events()
        .publish((symbol_short!("adm_init"),), (admin.clone(),));
}

pub fn emit_treasury_rotation_initiated(
    env: &Env,
    admin: &Address,
    new_address: &Address,
    deadline: u64,
) {
    env.events().publish(
        (symbol_short!("tr_rot_i"), admin.clone()),
        (new_address.clone(), deadline),
    );
}

pub fn emit_treasury_rotation_confirmed(env: &Env, admin: &Address, new_address: &Address) {
    env.events().publish(
        (symbol_short!("tr_rot_f"), admin.clone()),
        (new_address.clone(), env.ledger().timestamp()),
    );
}

pub fn treasury_rotation_cancelled(env: &Env, admin: &Address) {
    env.events()
        .publish((symbol_short!("tr_rot_cn"), admin.clone()), ());
}

// ── Upgrade events ──────────────────────────────────────────────────────────

/// Emitted when an admin schedules a WASM contract upgrade.
pub fn emit_upgrade_scheduled(env: &Env, admin: &Address, wasm_hash: &BytesN<32>) {
    env.events().publish(
        (symbol_short!("upg_sch"),),
        (admin.clone(), wasm_hash.clone(), env.ledger().timestamp()),
    );
}

/// Emitted when an admin cancels a pending WASM upgrade.
pub fn emit_upgrade_cancelled(env: &Env, admin: &Address, wasm_hash: &BytesN<32>) {
    env.events().publish(
        (symbol_short!("upg_can"),),
        (admin.clone(), wasm_hash.clone()),
    );
}

/// Emitted when a pending WASM upgrade is executed (contract code replaced).
pub fn emit_upgrade_executed(env: &Env, admin: &Address, wasm_hash: &BytesN<32>) {
    env.events().publish(
        (symbol_short!("upg_exe"),),
        (admin.clone(), wasm_hash.clone()),
    );
}

// ── Incident mode events ─────────────────────────────────────────────────────

/// Emitted when the admin enters coordinated incident mode (pause + maintenance).
#[contractevent]
pub struct IncidentModeEntered {
    pub admin: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Emitted when the admin exits coordinated incident mode (unpause + disable maintenance).
#[contractevent]
pub struct IncidentModeExited {
    pub admin: Address,
    pub timestamp: u64,
}

pub fn emit_incident_mode_entered(env: &Env, admin: &Address, reason: &String) {
    IncidentModeEntered {
        admin: admin.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_incident_mode_exited(env: &Env, admin: &Address) {
    IncidentModeExited {
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

// ── Arbiter (dispute-resolution) events ──────────────────────────────────────

/// Emitted when an admin registers a new dispute arbiter.
pub fn arbiter_registered(env: &Env, admin: &Address, arbiter: &Address) {
    env.events().publish(
        (symbol_short!("arb_reg"),),
        (admin.clone(), arbiter.clone(), env.ledger().timestamp()),
    );
}

/// Emitted when an admin revokes a previously registered dispute arbiter.
pub fn arbiter_revoked(env: &Env, admin: &Address, arbiter: &Address) {
    env.events().publish(
        (symbol_short!("arb_rvk"),),
        (admin.clone(), arbiter.clone(), env.ledger().timestamp()),
    );
}

// ── Backfill lifecycle events ────────────────────────────────────────────────

/// Emitted when a destructive backfill (e.g. `restore_from_backup`) begins.
/// While this flag is set, the contract refuses to schedule a WASM upgrade.
pub fn backfill_started(env: &Env, actor: &Address) {
    env.events().publish(
        (symbol_short!("bkf_sta"),),
        (actor.clone(), env.ledger().timestamp()),
    );
}

/// Emitted when a backfill finishes — flag is cleared and contracts are free
/// to migrate again.
pub fn backfill_finished(env: &Env, actor: &Address, restored_count: u32) {
    env.events().publish(
        (symbol_short!("bkf_end"),),
        (actor.clone(), restored_count, env.ledger().timestamp()),
    );
}

// ============================================================================
// Storage schema version and migration lifecycle events
// ============================================================================

/// Emitted when the contract records a new storage schema version.
///
/// # Fields
/// - `schema_version` – The new schema version number.
/// - `set_by`         – The admin who triggered the version bump.
/// - `timestamp`      – Ledger timestamp at emission time.
///
/// # Compatibility
/// This event is additive.  Indexers that only understand earlier versions
/// can safely ignore the `schema_version` payload.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct SchemaVersionSet {
    pub schema_version: u32,
    pub set_by: Address,
    pub timestamp: u64,
}

/// Emitted when a paginated schema migration begins (first page of a run).
///
/// # Fields
/// - `schema_from`    – Version being migrated away from.
/// - `schema_to`      – Version being migrated to.
/// - `initiated_by`   – Admin who started the migration.
/// - `timestamp`      – Ledger timestamp at emission time.
///
/// # Design invariant
/// Only one migration may be in progress at a time.  If a `MigrationStarted`
/// event is observed without a subsequent `MigrationCompleted` or
/// `MigrationRolledBack`, the migration is considered "in progress" and
/// writes to migrated entities must be rejected until it finishes.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationStarted {
    pub schema_from: u32,
    pub schema_to: u32,
    pub initiated_by: Address,
    pub timestamp: u64,
}

/// Emitted when every record has been migrated and the schema version is
/// committed to the new value.
///
/// # Fields
/// - `schema_from`      – The old schema version.
/// - `schema_to`        – The new, committed schema version.
/// - `records_migrated` – Total number of records processed.
/// - `completed_by`     – Admin who committed the final page.
/// - `timestamp`        – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationCompleted {
    pub schema_from: u32,
    pub schema_to: u32,
    pub records_migrated: u32,
    pub completed_by: Address,
    pub timestamp: u64,
}

/// Emitted when an in-progress migration is explicitly rolled back.
///
/// After this event the schema version is restored to `schema_from` and
/// storage is guaranteed to contain no partial new-schema records.
///
/// # Fields
/// - `schema_from`      – The version rolled back to.
/// - `schema_to`        – The version that was being migrated to.
/// - `rolled_back_by`   – Admin who performed the rollback.
/// - `timestamp`        – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationRolledBack {
    pub schema_from: u32,
    pub schema_to: u32,
    pub rolled_back_by: Address,
    pub timestamp: u64,
}

/// Emitted when a migration page fails partway through.
///
/// The migration is resumable: pass `next_offset` as the starting offset
/// for the next invocation.
///
/// # Fields
/// - `schema_from`      – Version being migrated from.
/// - `schema_to`        – Version being migrated to.
/// - `records_migrated` – Number of records successfully migrated so far.
/// - `next_offset`      – Offset to resume from on the next call.
/// - `reason`           – Machine-readable failure label (no PII).
/// - `timestamp`        – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct MigrationFailed {
    pub schema_from: u32,
    pub schema_to: u32,
    pub records_migrated: u32,
    pub next_offset: u32,
    pub reason: String,
    pub timestamp: u64,
}

// ── Emitter helpers ──────────────────────────────────────────────────────────

/// Emit a [`SchemaVersionSet`] event.
pub fn emit_schema_version_set(env: &Env, schema_version: u32, set_by: &Address) {
    SchemaVersionSet {
        schema_version,
        set_by: set_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationStarted`] event.
pub fn emit_migration_started(
    env: &Env,
    schema_from: u32,
    schema_to: u32,
    initiated_by: &Address,
) {
    MigrationStarted {
        schema_from,
        schema_to,
        initiated_by: initiated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationCompleted`] event.
pub fn emit_migration_completed(
    env: &Env,
    schema_from: u32,
    schema_to: u32,
    records_migrated: u32,
    completed_by: &Address,
) {
    MigrationCompleted {
        schema_from,
        schema_to,
        records_migrated,
        completed_by: completed_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationRolledBack`] event.
pub fn emit_migration_rolled_back(
    env: &Env,
    schema_from: u32,
    schema_to: u32,
    rolled_back_by: &Address,
) {
    MigrationRolledBack {
        schema_from,
        schema_to,
        rolled_back_by: rolled_back_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emit a [`MigrationFailed`] event.
pub fn emit_migration_failed(
    env: &Env,
    schema_from: u32,
    schema_to: u32,
    records_migrated: u32,
    next_offset: u32,
    reason: &str,
) {
    MigrationFailed {
        schema_from,
        schema_to,
        records_migrated,
        next_offset,
        reason: String::from_str(env, reason),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}
