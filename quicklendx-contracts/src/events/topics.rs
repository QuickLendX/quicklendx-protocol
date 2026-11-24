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
/// Topic for versioned `RepaymentAllocated` events (additive; does not replace `pay_rec`).
pub const TOPIC_REPAYMENT_ALLOCATED: &str = "repayment_allocated";
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
