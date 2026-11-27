//! Authoritative on-chain event schema table for QuickLendX.
//!
//! Every public event emitter in [`crate::events`] must have a corresponding
//! row in [`EVENT_SCHEMA`]. Off-chain indexers and documentation consume
//! `docs/events.schema.json`, which is generated directly from this table via
//! `scripts/gen-events-schema.sh`.
//!
//! Verification tests in [`test_events_schema`] enforce that:
//! 1. Every `emit_*` (and non-`emit_*` publisher) function in `src/events.rs`
//!    has a matching row in [`EVENT_SCHEMA`].
//! 2. Triggering each emitter produces an event whose topic, topic fields,
//!    payload format, and payload fields match its [`EVENT_SCHEMA`] row.

/// A single typed field in an event topic or payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventField {
    pub name: &'static str,
    pub ty: &'static str,
}

/// Schema definition for a single protocol event emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSchema {
    /// Name of the emitter function in `crate::events`.
    pub emitter: &'static str,
    /// Primary event topic symbol (topic[0]).
    pub topic: &'static str,
    /// Additional topic fields beyond `topic` (topic[1..]).
    pub topic_fields: &'static [EventField],
    /// Payload fields (named map keys for `contractevent`, positional elements for `tuple`).
    pub fields: &'static [EventField],
    /// Wire payload encoding: `"contractevent"`, `"tuple"`, or `"unit"`.
    pub format: &'static str,
}

const fn field(name: &'static str, ty: &'static str) -> EventField {
    EventField { name, ty }
}

/// Authoritative table of all events emitted by `quicklendx-contracts`.
pub const EVENT_SCHEMA: &[EventSchema] = &[
    EventSchema {
        emitter: "emit_ttl_extended",
        topic: "ttl_extended",
        topic_fields: &[],
        fields: &[field("kind", "String"), field("count", "u32")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_paused",
        topic: "paused",
        topic_fields: &[],
        fields: &[field("admin", "Address")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_unpaused",
        topic: "unpaused",
        topic_fields: &[],
        fields: &[field("admin", "Address")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_scan_item_failed",
        topic: "scan_item_failed",
        topic_fields: &[],
        fields: &[field("invoice_id", "BytesN<32>"), field("error_code", "u32")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_frozen",
        topic: "invoice_frozen",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("frozen_by", "Address"),
            field("reason", "String"),
            field("freeze_appeal_channel", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_uploaded",
        topic: "invoice_uploaded",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("amount", "i128"),
            field("currency", "Address"),
            field("due_date", "u64"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_verified",
        topic: "invoice_verified",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_cancelled",
        topic: "invoice_cancelled",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_metadata_updated",
        topic: "invoice_metadata_updated",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("line_item_count", "u32"),
            field("total_value", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_metadata_cleared",
        topic: "invoice_metadata_cleared",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_investor_verified",
        topic: "investor_verified",
        topic_fields: &[],
        fields: &[
            field("investor", "Address"),
            field("investment_limit", "i128"),
            field("verified_at", "u64"),
            field("verified_by", "Address"),
            field("tier", "u32"),
            field("risk_level", "u32"),
            field("risk_score", "u32"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_kyc_submitted",
        topic: "kyc_submitted",
        topic_fields: &[],
        fields: &[
            field("subject", "Address"),
            field("is_resubmit", "bool"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_kyc_verified",
        topic: "kyc_verified",
        topic_fields: &[],
        fields: &[
            field("subject", "Address"),
            field("admin", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_kyc_rejected",
        topic: "kyc_rejected",
        topic_fields: &[],
        fields: &[
            field("subject", "Address"),
            field("admin", "Address"),
            field("reason", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_kyc_revoked",
        topic: "kyc_revoked",
        topic_fields: &[],
        fields: &[
            field("investor", "Address"),
            field("admin", "Address"),
            field("reason", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_investor_frozen",
        topic: "investor_frozen",
        topic_fields: &[],
        fields: &[
            field("investor", "Address"),
            field("admin", "Address"),
            field("reason", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_investor_unfrozen",
        topic: "investor_unfrozen",
        topic_fields: &[],
        fields: &[
            field("investor", "Address"),
            field("admin", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_settled",
        topic: "invoice_settled",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("amount", "i128"),
            field("ledger", "u32"),
            field("business", "Address"),
            field("investor", "Address"),
            field("investor_return", "i128"),
            field("platform_fee", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_partial_payment",
        topic: "partial_payment",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("payment_amount", "i128"),
            field("total_paid", "i128"),
            field("progress", "u32"),
            field("transaction_id", "String"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_payment_recorded",
        topic: "payment_recorded",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("payer", "Address"),
            field("amount", "i128"),
            field("transaction_id", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_repayment_allocated",
        topic: "repayment_allocated",
        topic_fields: &[],
        fields: &[
            field("schema_version", "u32"),
            field("operation_id", "BytesN<32>"),
            field("phase", "TransitionPhase"),
            field("invoice_id", "BytesN<32>"),
            field("payer", "Address"),
            field("applied_principal", "i128"),
            field("applied_investor_profit", "i128"),
            field("applied_platform_fee", "i128"),
            field("applied_late_penalty", "i128"),
            field("cumulative_principal", "i128"),
            field("cumulative_investor_profit", "i128"),
            field("cumulative_platform_fee", "i128"),
            field("cumulative_late_penalty", "i128"),
            field("total_paid", "i128"),
            field("total_due", "i128"),
            field("fee_bps", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_settled_final",
        topic: "invoice_settled_final",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("investor", "Address"),
            field("total_paid", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_expired",
        topic: "invoice_expired",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("due_date", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_defaulted",
        topic: "invoice_defaulted",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("investor", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_funded",
        topic: "invoice_funded",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("amount", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_insurance_added",
        topic: "insurance_added",
        topic_fields: &[],
        fields: &[
            field("investment_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("provider", "Address"),
            field("coverage_percentage", "u32"),
            field("coverage_amount", "i128"),
            field("premium_amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_insurance_premium_collected",
        topic: "insurance_premium_collected",
        topic_fields: &[],
        fields: &[
            field("investment_id", "BytesN<32>"),
            field("provider", "Address"),
            field("premium_amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_insurance_claimed",
        topic: "insurance_claimed",
        topic_fields: &[],
        fields: &[
            field("investment_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("provider", "Address"),
            field("coverage_amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_platform_fee_updated",
        topic: "platform_fee_updated",
        topic_fields: &[],
        fields: &[
            field("fee_bps", "u32"),
            field("updated_at", "u64"),
            field("updated_by", "Address"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_fee_structure_updated",
        topic: "fee_structure_updated",
        topic_fields: &[],
        fields: &[
            field("fee_type", "FeeType"),
            field("old_fee_bps", "u32"),
            field("new_fee_bps", "u32"),
            field("updated_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_platform_fee_routed",
        topic: "platform_fee_routed",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("recipient", "Address"),
            field("fee_amount", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_platform_fee_config_updated",
        topic: "platform_fee_config_updated",
        topic_fields: &[],
        fields: &[
            field("old_fee_bps", "u32"),
            field("new_fee_bps", "u32"),
            field("updated_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_treasury_configured",
        topic: "treasury_configured",
        topic_fields: &[],
        fields: &[
            field("treasury_address", "Address"),
            field("configured_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_escrow_created",
        topic: "escrow_created",
        topic_fields: &[],
        fields: &[
            field("escrow_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("business", "Address"),
            field("amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_escrow_released",
        topic: "escrow_released",
        topic_fields: &[],
        fields: &[
            field("escrow_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_escrow_refunded",
        topic: "escrow_refunded",
        topic_fields: &[],
        fields: &[
            field("escrow_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_investment_withdrawn",
        topic: "investment_withdrawn",
        topic_fields: &[],
        fields: &[
            field("investment_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_bid_placed",
        topic: "bid_placed",
        topic_fields: &[],
        fields: &[
            field("bid_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("bid_amount", "i128"),
            field("expected_return", "i128"),
            field("timestamp", "u64"),
            field("expiration_timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_bid_withdrawn",
        topic: "bid_withdrawn",
        topic_fields: &[],
        fields: &[
            field("bid_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("bid_amount", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_bid_cancelled",
        topic: "bid_cancelled",
        topic_fields: &[],
        fields: &[
            field("bid_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("bid_amount", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_bid_accepted",
        topic: "bid_accepted",
        topic_fields: &[],
        fields: &[
            field("bid_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("business", "Address"),
            field("bid_amount", "i128"),
            field("expected_return", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_bid_expired",
        topic: "bid_expired",
        topic_fields: &[],
        fields: &[
            field("bid_id", "BytesN<32>"),
            field("invoice_id", "BytesN<32>"),
            field("investor", "Address"),
            field("bid_amount", "i128"),
            field("expiration_timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_backup_created",
        topic: "backup_created",
        topic_fields: &[],
        fields: &[
            field("backup_id", "BytesN<32>"),
            field("invoice_count", "u32"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_backup_restored",
        topic: "backup_restored",
        topic_fields: &[],
        fields: &[
            field("backup_id", "BytesN<32>"),
            field("invoice_count", "u32"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_backup_validated",
        topic: "backup_validated",
        topic_fields: &[],
        fields: &[
            field("backup_id", "BytesN<32>"),
            field("success", "bool"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_backup_archived",
        topic: "backup_archived",
        topic_fields: &[],
        fields: &[field("backup_id", "BytesN<32>"), field("timestamp", "u64")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_retention_policy_updated",
        topic: "retention_policy_updated",
        topic_fields: &[],
        fields: &[
            field("max_backups", "u32"),
            field("max_age_seconds", "u64"),
            field("auto_cleanup_enabled", "bool"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_backups_cleaned",
        topic: "backups_cleaned",
        topic_fields: &[],
        fields: &[field("removed_count", "u32"), field("timestamp", "u64")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_audit_validation",
        topic: "audit_validation",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("is_valid", "bool"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_audit_query",
        topic: "audit_query",
        topic_fields: &[],
        fields: &[field("query_type", "OpType"), field("result_count", "u32")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_category_updated",
        topic: "invoice_category_updated",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("old_category", "InvoiceCategory"),
            field("new_category", "InvoiceCategory"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_tag_added",
        topic: "invoice_tag_added",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("tag", "String"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_tag_removed",
        topic: "invoice_tag_removed",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("business", "Address"),
            field("tag", "String"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_dispute_created",
        topic: "dispute_created",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("created_by", "Address"),
            field("reason", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_dispute_under_review",
        topic: "dispute_under_review",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("reviewed_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_dispute_resolved",
        topic: "dispute_resolved",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("resolved_by", "Address"),
            field("resolution", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_dispute_rejected",
        topic: "dispute_rejected",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("rejected_by", "Address"),
            field("reason", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_profit_fee_breakdown",
        topic: "profit_fee_breakdown",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("investment_amount", "i128"),
            field("payment_amount", "i128"),
            field("gross_profit", "i128"),
            field("platform_fee", "i128"),
            field("investor_return", "i128"),
            field("fee_bps_applied", "i128"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_bid_ttl_updated",
        topic: "bid_ttl_updated",
        topic_fields: &[],
        fields: &[
            field("old_days", "u64"),
            field("new_days", "u64"),
            field("admin", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_bid_expiry_grace_updated",
        topic: "bid_expiry_grace_updated",
        topic_fields: &[],
        fields: &[
            field("old_seconds", "u64"),
            field("new_seconds", "u64"),
            field("admin", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_emergency_withdrawal_initiated",
        topic: "emergency_withdrawal_initiated",
        topic_fields: &[],
        fields: &[
            field("token", "Address"),
            field("amount", "i128"),
            field("target", "Address"),
            field("unlock_at", "u64"),
            field("admin", "Address"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_emergency_withdrawal_executed",
        topic: "emergency_withdrawal_executed",
        topic_fields: &[],
        fields: &[
            field("token", "Address"),
            field("amount", "i128"),
            field("target", "Address"),
            field("admin", "Address"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_emergency_withdrawal_cancelled",
        topic: "emergency_withdrawal_cancelled",
        topic_fields: &[],
        fields: &[
            field("token", "Address"),
            field("amount", "i128"),
            field("target", "Address"),
            field("admin", "Address"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_admin_set",
        topic: "admin_set",
        topic_fields: &[],
        fields: &[field("admin", "Address"), field("timestamp", "u64")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_admin_transferred",
        topic: "adm_trf",
        topic_fields: &[],
        fields: &[
            field("old_admin", "Address"),
            field("new_admin", "Address"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_admin_transfer_initiated",
        topic: "adm_req",
        topic_fields: &[],
        fields: &[
            field("current_admin", "Address"),
            field("pending_admin", "Address"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_admin_transfer_cancelled",
        topic: "adm_cnl",
        topic_fields: &[],
        fields: &[
            field("current_admin", "Address"),
            field("pending_admin", "Address"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_admin_two_step_updated",
        topic: "adm_2st",
        topic_fields: &[],
        fields: &[
            field("admin", "Address"),
            field("enabled", "bool"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_revenue_distributed",
        topic: "revenue_distributed",
        topic_fields: &[],
        fields: &[
            field("period", "u64"),
            field("treasury_amount", "i128"),
            field("developer_amount", "i128"),
            field("platform_amount", "i128"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_invoice_status_updated",
        topic: "invoice_status_updated",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("status", "InvoiceStatus"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_protocol_initialized",
        topic: "protocol_initialized",
        topic_fields: &[],
        fields: &[
            field("admin", "Address"),
            field("treasury", "Address"),
            field("fee_bps", "u32"),
            field("min_invoice_amount", "i128"),
            field("max_due_date_days", "u64"),
            field("grace_period_seconds", "u64"),
            field("backfill_max_batch_size", "u32"),
            field("corridors", "Vec<Address>"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_admin_initialized",
        topic: "adm_init",
        topic_fields: &[],
        fields: &[field("admin", "Address")],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_treasury_rotation_initiated",
        topic: "tr_rot_i",
        topic_fields: &[field("admin", "Address")],
        fields: &[field("new_address", "Address"), field("deadline", "u64")],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_treasury_rotation_confirmed",
        topic: "tr_rot_f",
        topic_fields: &[field("admin", "Address")],
        fields: &[field("new_address", "Address"), field("timestamp", "u64")],
        format: "tuple",
    },
    EventSchema {
        emitter: "treasury_rotation_cancelled",
        topic: "tr_rot_cn",
        topic_fields: &[field("admin", "Address")],
        fields: &[],
        format: "unit",
    },
    EventSchema {
        emitter: "emit_upgrade_scheduled",
        topic: "upg_sch",
        topic_fields: &[],
        fields: &[
            field("admin", "Address"),
            field("wasm_hash", "BytesN<32>"),
            field("timestamp", "u64"),
            field("executable_at", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_upgrade_cancelled",
        topic: "upg_can",
        topic_fields: &[],
        fields: &[field("admin", "Address"), field("wasm_hash", "BytesN<32>")],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_upgrade_executed",
        topic: "upg_exe",
        topic_fields: &[],
        fields: &[field("admin", "Address"), field("wasm_hash", "BytesN<32>")],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_incident_mode_entered",
        topic: "incident_mode_entered",
        topic_fields: &[],
        fields: &[
            field("admin", "Address"),
            field("reason", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_incident_mode_exited",
        topic: "incident_mode_exited",
        topic_fields: &[],
        fields: &[field("admin", "Address"), field("timestamp", "u64")],
        format: "contractevent",
    },
    EventSchema {
        emitter: "arbiter_registered",
        topic: "arb_reg",
        topic_fields: &[],
        fields: &[
            field("admin", "Address"),
            field("arbiter", "Address"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "arbiter_revoked",
        topic: "arb_rvk",
        topic_fields: &[],
        fields: &[
            field("admin", "Address"),
            field("arbiter", "Address"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "dispute_arbiter_voted",
        topic: "dsp_vote",
        topic_fields: &[],
        fields: &[
            field("invoice_id", "BytesN<32>"),
            field("arbiter", "Address"),
            field("outcome_code", "u32"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "dispute_quorum_updated",
        topic: "dsp_qrm",
        topic_fields: &[],
        fields: &[
            field("admin", "Address"),
            field("quorum", "u32"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "backfill_started",
        topic: "bkf_sta",
        topic_fields: &[],
        fields: &[field("actor", "Address"), field("timestamp", "u64")],
        format: "tuple",
    },
    EventSchema {
        emitter: "backfill_finished",
        topic: "bkf_end",
        topic_fields: &[],
        fields: &[
            field("actor", "Address"),
            field("restored_count", "u32"),
            field("timestamp", "u64"),
        ],
        format: "tuple",
    },
    EventSchema {
        emitter: "emit_schema_version_set",
        topic: "schema_version_set",
        topic_fields: &[],
        fields: &[
            field("schema_version", "u32"),
            field("set_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_migration_started",
        topic: "migration_started",
        topic_fields: &[],
        fields: &[
            field("schema_from", "u32"),
            field("schema_to", "u32"),
            field("initiated_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_migration_completed",
        topic: "migration_completed",
        topic_fields: &[],
        fields: &[
            field("schema_from", "u32"),
            field("schema_to", "u32"),
            field("records_migrated", "u32"),
            field("completed_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_migration_rolled_back",
        topic: "migration_rolled_back",
        topic_fields: &[],
        fields: &[
            field("schema_from", "u32"),
            field("schema_to", "u32"),
            field("rolled_back_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_migration_failed",
        topic: "migration_failed",
        topic_fields: &[],
        fields: &[
            field("schema_from", "u32"),
            field("schema_to", "u32"),
            field("records_migrated", "u32"),
            field("next_offset", "u32"),
            field("reason", "String"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
    EventSchema {
        emitter: "emit_migration_step_executed",
        topic: "migration_step_executed",
        topic_fields: &[],
        fields: &[
            field("schema_from", "u32"),
            field("schema_to", "u32"),
            field("offset", "u32"),
            field("processed", "u32"),
            field("total_records", "u32"),
            field("executed_by", "Address"),
            field("timestamp", "u64"),
        ],
        format: "contractevent",
    },
];

#[cfg(test)]
mod test_events_schema {
    extern crate std;

    use super::*;
    use crate::audit::OpType;
    use crate::events;
    use crate::fees::FeeType;
    use crate::payments::{Escrow, EscrowStatus};
    use crate::types::{
        Bid, BidStatus, DisputeResolution, InvestorFreezeReason, Invoice, InvoiceCategory,
        InvoiceMetadata, InvoiceStatus, PlatformFeeConfig,
    };
    use crate::verification::{
        BusinessVerificationStatus, InvestorRiskLevel, InvestorTier, InvestorVerification,
    };
    use soroban_sdk::testutils::{Address as _, Events as _};
    use soroban_sdk::{contract, Address, BytesN, Env, Map, String, Symbol, TryFromVal, Val, Vec};
    use std::collections::BTreeSet;
    use std::vec::Vec as StdVec;

    #[contract]
    struct DummyEventContract;

    fn trigger_emitter(env: &Env, emitter: &str) {
        let id = BytesN::from_array(env, &[1u8; 32]);
        let id2 = BytesN::from_array(env, &[2u8; 32]);
        let addr1 = Address::generate(env);
        let addr2 = Address::generate(env);
        let addr3 = Address::generate(env);
        let s = String::from_str(env, "sample");
        let invoice = Invoice::new(
            env,
            addr1.clone(),
            1_000,
            addr2.clone(),
            env.ledger().timestamp() + 86_400,
            s.clone(),
            InvoiceCategory::Services,
            Vec::new(env),
        )
        .expect("fixture invoice");
        let metadata = InvoiceMetadata {
            customer_name: s.clone(),
            customer_address: s.clone(),
            tax_id: s.clone(),
            line_items: Vec::new(env),
            notes: s.clone(),
        };
        let verification = InvestorVerification {
            investor: addr1.clone(),
            status: BusinessVerificationStatus::Verified,
            verified_at: Some(100),
            verified_by: Some(addr2.clone()),
            kyc_data: s.clone(),
            investment_limit: 5_000,
            submitted_at: 50,
            tier: InvestorTier::Gold,
            risk_level: InvestorRiskLevel::Low,
            risk_score: 10,
            total_invested: 0,
            total_returns: 0,
            successful_investments: 0,
            defaulted_investments: 0,
            last_activity: 100,
            rejection_reason: None,
            compliance_notes: None,
        };
        let fee_config = PlatformFeeConfig {
            fee_bps: 200,
            treasury_address: Some(addr2.clone()),
            updated_at: 100,
            updated_by: addr1.clone(),
        };
        let escrow = Escrow {
            escrow_id: id.clone(),
            invoice_id: id2.clone(),
            investor: addr1.clone(),
            business: addr2.clone(),
            amount: 1_000,
            currency: addr3.clone(),
            created_at: 100,
            status: EscrowStatus::Held,
        };
        let bid = Bid {
            bid_id: id.clone(),
            invoice_id: id2.clone(),
            investor: addr1.clone(),
            bid_amount: 900,
            expected_return: 1_000,
            timestamp: 100,
            status: BidStatus::Placed,
            expiration_timestamp: 200,
        };

        match emitter {
            "emit_ttl_extended" => events::emit_ttl_extended(env, &s, 1),
            "emit_paused" => events::emit_paused(env, &addr1),
            "emit_unpaused" => events::emit_unpaused(env, &addr1),
            "emit_scan_item_failed" => events::emit_scan_item_failed(env, &id, 42),
            "emit_invoice_frozen" => {
                events::emit_invoice_frozen(env, &id, &addr1, "admin_action")
            }
            "emit_invoice_uploaded" => events::emit_invoice_uploaded(env, &invoice),
            "emit_invoice_verified" => events::emit_invoice_verified(env, &invoice),
            "emit_invoice_cancelled" => events::emit_invoice_cancelled(env, &invoice),
            "emit_invoice_metadata_updated" => {
                events::emit_invoice_metadata_updated(env, &invoice, &metadata)
            }
            "emit_invoice_metadata_cleared" => {
                events::emit_invoice_metadata_cleared(env, &invoice)
            }
            "emit_investor_verified" => events::emit_investor_verified(env, &verification),
            "emit_kyc_submitted" => events::emit_kyc_submitted(env, &addr1, false),
            "emit_kyc_verified" => events::emit_kyc_verified(env, &addr1, &addr2),
            "emit_kyc_rejected" => events::emit_kyc_rejected(env, &addr1, &addr2, &s),
            "emit_kyc_revoked" => events::emit_kyc_revoked(env, &addr1, &addr2, &s),
            "emit_investor_frozen" => events::emit_investor_frozen(
                env,
                &addr1,
                &addr2,
                &InvestorFreezeReason::AdminAction,
            ),
            "emit_investor_unfrozen" => events::emit_investor_unfrozen(env, &addr1, &addr2),
            "emit_invoice_settled" => events::emit_invoice_settled(env, &invoice, 950, 50),
            "emit_partial_payment" => {
                events::emit_partial_payment(env, &invoice, 250, 250, 25, s.clone())
            }
            "emit_payment_recorded" => {
                events::emit_payment_recorded(env, &id, &addr1, 250, s.clone())
            }
            "emit_repayment_allocated" => events::emit_repayment_allocated(
                env,
                id.clone(),
                &id2,
                &addr1,
                100,
                10,
                2,
                0,
                100,
                10,
                2,
                0,
                112,
                1_000,
                200,
            ),
            "emit_invoice_settled_final" => {
                events::emit_invoice_settled_final(env, &id, &addr1, &addr2, 1_000)
            }
            "emit_invoice_expired" => events::emit_invoice_expired(env, &invoice),
            "emit_invoice_defaulted" => events::emit_invoice_defaulted(env, &invoice),
            "emit_invoice_funded" => events::emit_invoice_funded(env, &id, &addr1, 1_000),
            "emit_insurance_added" => {
                events::emit_insurance_added(env, &id, &id2, &addr1, &addr2, 80, 800, 40)
            }
            "emit_insurance_premium_collected" => {
                events::emit_insurance_premium_collected(env, &id, &addr1, 40)
            }
            "emit_insurance_claimed" => {
                events::emit_insurance_claimed(env, &id, &id2, &addr1, 800)
            }
            "emit_platform_fee_updated" => events::emit_platform_fee_updated(env, &fee_config),
            "emit_fee_structure_updated" => events::emit_fee_structure_updated(
                env,
                &FeeType::Platform,
                100,
                200,
                &addr1,
            ),
            "emit_platform_fee_routed" => {
                events::emit_platform_fee_routed(env, &id, &addr1, 50)
            }
            "emit_platform_fee_config_updated" => {
                events::emit_platform_fee_config_updated(env, 100, 200, &addr1)
            }
            "emit_treasury_configured" => {
                events::emit_treasury_configured(env, &addr1, &addr2)
            }
            "emit_escrow_created" => events::emit_escrow_created(env, &escrow),
            "emit_escrow_released" => {
                events::emit_escrow_released(env, &id, &id2, &addr1, 1_000)
            }
            "emit_escrow_refunded" => {
                events::emit_escrow_refunded(env, &id, &id2, &addr1, 1_000)
            }
            "emit_investment_withdrawn" => {
                events::emit_investment_withdrawn(env, &id, &id2, &addr1, 1_000)
            }
            "emit_bid_placed" => events::emit_bid_placed(env, &bid),
            "emit_bid_withdrawn" => events::emit_bid_withdrawn(env, &bid),
            "emit_bid_cancelled" => events::emit_bid_cancelled(env, &bid),
            "emit_bid_accepted" => events::emit_bid_accepted(env, &bid, &id2, &addr2),
            "emit_bid_expired" => events::emit_bid_expired(env, &bid),
            "emit_backup_created" => events::emit_backup_created(env, &id, 5),
            "emit_backup_restored" => events::emit_backup_restored(env, &id, 5),
            "emit_backup_validated" => events::emit_backup_validated(env, &id, true),
            "emit_backup_archived" => events::emit_backup_archived(env, &id),
            "emit_retention_policy_updated" => {
                events::emit_retention_policy_updated(env, 10, 86_400, true)
            }
            "emit_backups_cleaned" => events::emit_backups_cleaned(env, 3),
            "emit_audit_validation" => events::emit_audit_validation(env, &id, true),
            "emit_audit_query" => {
                events::emit_audit_query(env, OpType::InvoiceUploaded, 1)
            }
            "emit_invoice_category_updated" => events::emit_invoice_category_updated(
                env,
                &id,
                &addr1,
                &InvoiceCategory::Services,
                &InvoiceCategory::Products,
            ),
            "emit_invoice_tag_added" => events::emit_invoice_tag_added(env, &id, &addr1, &s),
            "emit_invoice_tag_removed" => {
                events::emit_invoice_tag_removed(env, &id, &addr1, &s)
            }
            "emit_dispute_created" => events::emit_dispute_created(env, &id, &addr1, &s),
            "emit_dispute_under_review" => {
                events::emit_dispute_under_review(env, &id, &addr1)
            }
            "emit_dispute_resolved" => events::emit_dispute_resolved(env, &id, &addr1, &s),
            "emit_dispute_rejected" => events::emit_dispute_rejected(env, &id, &addr1, &s),
            "emit_profit_fee_breakdown" => {
                events::emit_profit_fee_breakdown(env, &id, 900, 1_000, 100, 2, 998, 200)
            }
            "emit_bid_ttl_updated" => events::emit_bid_ttl_updated(env, 7, 14, &addr1),
            "emit_bid_expiry_grace_updated" => {
                events::emit_bid_expiry_grace_updated(env, 0, 3_600, &addr1)
            }
            "emit_emergency_withdrawal_initiated" => {
                events::emit_emergency_withdrawal_initiated(
                    env,
                    addr1.clone(),
                    500,
                    addr2.clone(),
                    1_000,
                    addr3.clone(),
                )
            }
            "emit_emergency_withdrawal_executed" => {
                events::emit_emergency_withdrawal_executed(
                    env,
                    addr1.clone(),
                    500,
                    addr2.clone(),
                    addr3.clone(),
                )
            }
            "emit_emergency_withdrawal_cancelled" => {
                events::emit_emergency_withdrawal_cancelled(
                    env,
                    addr1.clone(),
                    500,
                    addr2.clone(),
                    addr3.clone(),
                )
            }
            "emit_admin_set" => events::emit_admin_set(env, &addr1),
            "emit_admin_transferred" => events::emit_admin_transferred(env, &addr1, &addr2),
            "emit_admin_transfer_initiated" => {
                events::emit_admin_transfer_initiated(env, &addr1, &addr2)
            }
            "emit_admin_transfer_cancelled" => {
                events::emit_admin_transfer_cancelled(env, &addr1, &addr2)
            }
            "emit_admin_two_step_updated" => {
                events::emit_admin_two_step_updated(env, &addr1, true)
            }
            "emit_revenue_distributed" => {
                events::emit_revenue_distributed(env, 1, 100, 200, 300)
            }
            "emit_invoice_status_updated" => {
                events::emit_invoice_status_updated(env, id, InvoiceStatus::Verified)
            }
            "emit_protocol_initialized" => events::emit_protocol_initialized(
                env,
                &addr1,
                &addr2,
                200,
                100,
                365,
                86_400,
                50,
                &Vec::new(env),
            ),
            "emit_admin_initialized" => events::emit_admin_initialized(env, &addr1),
            "emit_treasury_rotation_initiated" => {
                events::emit_treasury_rotation_initiated(env, &addr1, &addr2, 1_000)
            }
            "emit_treasury_rotation_confirmed" => {
                events::emit_treasury_rotation_confirmed(env, &addr1, &addr2)
            }
            "treasury_rotation_cancelled" => {
                events::treasury_rotation_cancelled(env, &addr1)
            }
            "emit_upgrade_scheduled" => {
                events::emit_upgrade_scheduled(env, &addr1, &id, 1_000)
            }
            "emit_upgrade_cancelled" => events::emit_upgrade_cancelled(env, &addr1, &id),
            "emit_upgrade_executed" => events::emit_upgrade_executed(env, &addr1, &id),
            "emit_incident_mode_entered" => {
                events::emit_incident_mode_entered(env, &addr1, &s)
            }
            "emit_incident_mode_exited" => events::emit_incident_mode_exited(env, &addr1),
            "arbiter_registered" => events::arbiter_registered(env, &addr1, &addr2),
            "arbiter_revoked" => events::arbiter_revoked(env, &addr1, &addr2),
            "dispute_arbiter_voted" => events::dispute_arbiter_voted(
                env,
                &id,
                &addr1,
                DisputeResolution::RefundInvestor,
            ),
            "dispute_quorum_updated" => events::dispute_quorum_updated(env, &addr1, 2),
            "backfill_started" => events::backfill_started(env, &addr1),
            "backfill_finished" => events::backfill_finished(env, &addr1, 10),
            "emit_schema_version_set" => events::emit_schema_version_set(env, 2, &addr1),
            "emit_migration_started" => events::emit_migration_started(env, 1, 2, &addr1),
            "emit_migration_completed" => {
                events::emit_migration_completed(env, 1, 2, 10, &addr1)
            }
            "emit_migration_rolled_back" => {
                events::emit_migration_rolled_back(env, 1, 2, &addr1)
            }
            "emit_migration_failed" => {
                events::emit_migration_failed(env, 1, 2, 5, 5, "timeout")
            }
            "emit_migration_step_executed" => {
                events::emit_migration_step_executed(env, 1, 2, 0, 5, 10, &addr1)
            }
            other => panic!("Unhandled emitter in trigger_emitter: {other}"),
        }
    }

    #[test]
    fn test_every_emitter_in_events_rs_has_schema_row() {
        let source = include_str!("events.rs");
        let mut emit_fns = BTreeSet::new();

        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("pub fn ") {
                if let Some(name) = rest.split('(').next() {
                    let name = name.trim();
                    if !name.is_empty() {
                        emit_fns.insert(name);
                    }
                }
            }
        }

        let mut schema_emitters = BTreeSet::new();
        for entry in EVENT_SCHEMA {
            assert!(
                schema_emitters.insert(entry.emitter),
                "Duplicate emitter in EVENT_SCHEMA: {}",
                entry.emitter
            );
        }

        let missing: StdVec<&str> = emit_fns.difference(&schema_emitters).copied().collect();
        assert!(
            missing.is_empty(),
            "Emitter functions in src/events.rs missing from EVENT_SCHEMA: {:?}",
            missing
        );

        let stale: StdVec<&str> = schema_emitters.difference(&emit_fns).copied().collect();
        assert!(
            stale.is_empty(),
            "Stale emitters in EVENT_SCHEMA not found in src/events.rs: {:?}",
            stale
        );
    }

    #[test]
    fn test_all_schema_emitters_match_emitted_events() {
        for entry in EVENT_SCHEMA {
            let env = Env::default();
            let contract_id = env.register(DummyEventContract, ());

            env.as_contract(&contract_id, || {
                trigger_emitter(&env, entry.emitter);
            });

            let emitted = env.events().all();
            assert_eq!(
                emitted.len(),
                1,
                "Emitter '{}' expected to emit 1 event, got {}",
                entry.emitter,
                emitted.len()
            );

            let (_contract, topics, data) = emitted.get(0).unwrap();
            let expected_topic_len = (1 + entry.topic_fields.len()) as u32;
            assert_eq!(
                topics.len(),
                expected_topic_len,
                "Emitter '{}' topic length mismatch: expected {}, got {}",
                entry.emitter,
                expected_topic_len,
                topics.len()
            );

            let actual_topic = Symbol::try_from_val(&env, &topics.get(0).unwrap())
                .unwrap_or_else(|_| panic!("Emitter '{}' topic[0] is not a Symbol", entry.emitter));
            let expected_topic = Symbol::new(&env, entry.topic);
            assert_eq!(
                actual_topic, expected_topic,
                "Emitter '{}' topic mismatch: expected '{}', got {:?}",
                entry.emitter, entry.topic, actual_topic
            );

            match entry.format {
                "contractevent" => {
                    let map = Map::<Symbol, Val>::try_from_val(&env, &data).unwrap_or_else(|_| {
                        panic!(
                            "Emitter '{}' declared as 'contractevent' but payload is not a Map<Symbol, Val>",
                            entry.emitter
                        )
                    });
                    assert_eq!(
                        map.len(),
                        entry.fields.len() as u32,
                        "Emitter '{}' field count mismatch: schema has {}, emitted map has {}",
                        entry.emitter,
                        entry.fields.len(),
                        map.len()
                    );
                    for f in entry.fields {
                        let key = Symbol::new(&env, f.name);
                        assert!(
                            map.contains_key(key),
                            "Emitter '{}' missing expected field '{}' in emitted payload",
                            entry.emitter,
                            f.name
                        );
                    }
                }
                "tuple" => {
                    let vec = Vec::<Val>::try_from_val(&env, &data).unwrap_or_else(|_| {
                        panic!(
                            "Emitter '{}' declared as 'tuple' but payload is not a Vec<Val>",
                            entry.emitter
                        )
                    });
                    assert_eq!(
                        vec.len(),
                        entry.fields.len() as u32,
                        "Emitter '{}' tuple arity mismatch: schema has {}, emitted tuple has {}",
                        entry.emitter,
                        entry.fields.len(),
                        vec.len()
                    );
                }
                "unit" => {
                    assert!(
                        entry.fields.is_empty(),
                        "Emitter '{}' declared as 'unit' must have empty fields",
                        entry.emitter
                    );
                    assert!(
                        data.is_void(),
                        "Emitter '{}' declared as 'unit' must emit void payload",
                        entry.emitter
                    );
                }
                other => panic!(
                    "Emitter '{}' has unknown format '{}'",
                    entry.emitter, other
                ),
            }
        }
    }
}
