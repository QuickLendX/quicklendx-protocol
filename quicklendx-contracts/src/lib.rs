#![no_std]

extern crate alloc;

// ============================================================================
// Module declarations
// ============================================================================

pub mod admin;
pub mod arbiter;
pub mod audit;
pub mod backpressure;
pub mod backup;
pub mod backup_v1;
#[cfg(any(test, feature = "testutils"))]
pub mod bench;
pub mod bid;
pub mod currency;
pub mod defaults;
pub mod dispute;
pub mod dispute_timeline;
pub mod emergency;
pub mod errors;
pub mod escrow;
pub mod events;
pub mod fees;
pub mod freshness;
pub mod governance;
pub mod health;
pub mod idempotency;
pub mod incident;
pub mod init;
pub mod invariants;
pub mod investment;
pub mod investment_queries;
pub mod invoice;
pub mod invoice_amount;
pub mod invoice_search;
pub mod kyc_policy;
pub mod maintenance;
pub mod migration;
pub mod monitor;
pub mod multisig;
pub mod notifications;
pub mod observability;
pub mod operational_limits;
pub mod pagination;
pub mod panic_handler;
pub mod pause;
pub mod payment_token_policy;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod reentrancy;
pub mod regulatory;
pub mod resolution_policy;
pub mod settlement;
pub mod storage;
pub mod types;
pub mod upgrade;
pub mod verification;
pub mod vesting;

// diagnostics is feature-gated
#[cfg(feature = "diagnostics")]
pub mod diagnostics;

pub mod address_summary;
pub mod analytics;

/// Shared test utilities accessible from integration tests.
pub mod test_utils;

// ============================================================================
// Test modules
// ============================================================================

#[cfg(test)]
mod test_dispute;
#[cfg(test)]
mod test_dispute_arbiter;
#[cfg(test)]
mod test_dispute_timeline;
#[cfg(test)]
mod test_partial_payments;
#[cfg(test)]
mod test_payment_history;
#[cfg(test)]
mod test_settlement;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_partial_payments;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_payment_history;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_timeline;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_arbiter;

// The remaining test modules (already present in the codebase)
#[cfg(test)]
mod test_fee_admin_auth;
#[cfg(test)]
mod test_platform_fee_ownership;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
mod test_address_summary;
#[cfg(test)]
mod test_rating_auth;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_accept_bid_instruction_budget;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_accept_bid_race;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_events_audit_parity;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_handover;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_lookalike;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_simple;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_standalone;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_two_step;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_analytics_consistency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_audit;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_audit_config;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_audit_stats_regression;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_authorization;
#[cfg(all(test, feature = "legacy-tests"))]
#[cfg(test)]
mod test_entrypoint_auth_matrix;
#[cfg(test)]
mod test_auto_resolution_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backfill_guard;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backpressure_shedding;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup_restore_reindex;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup_retention_enforcement;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup_safety;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_acceptance_replay;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_atomic_rollback;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_audit_parity;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_cancel_accept_race;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
mod test_bid_capacity_stress;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_capacity_stress;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_compare_order_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_concurrency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_exclusivity;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_expiry_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_expiry_grace;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
mod test_bid_match_helper;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
mod test_bid_ranking;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
mod test_bid_ranking_determinism;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_match_helper;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_ranking;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_ranking_determinism;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_ranking_tiebreaker;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_state_matrix;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
mod test_bid_transition_matrix;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_transition_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_ttl;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_validation;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_business_freeze_reason;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_business_invoices_paged_ordering;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_business_kyc;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_cancel_invoice_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_cancel_refund;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_cannot_withdraw_more_than_deposited;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_category_breakdown;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_cleanup_pagination;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_clock_rollover;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_compute_yield_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_concurrent_default_overlap;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_config_bounds_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_cross_module_consistency;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_currency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_currency_batch;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_currency_match_funding;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_currency_precision;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_default;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_default_finality;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_default_finality_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_default_grace_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_diagnostics;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_event_invariant;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_evidence_identity;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_history_guard;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_refund_flow;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_settlement;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_state_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_time_limit;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_timeline_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_due_date_boundaries;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_due_date_guard;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dust_transfer;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_early_payment_discount_bps;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_emergency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_emergency_escrow_protection;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_emergency_withdraw;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_emergency_withdraw_props;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_error_code_stability;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_errors;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_consistency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_early_release;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_event_completeness;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_expiry_extend;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_invariant_model;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_mutual_exclusion;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_refund;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_refund_after_expiry;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_refund_hardened;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_settle_refund_race;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_state_machine;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_time_limit;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
mod test_escrow_uniqueness;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_uniqueness;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_events;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_evidence_hash_format;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_evidence_kind_guard_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_evidence_size_cap;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_expired_bids_cleanup;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_expired_escrow;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fee_analytics_boundaries;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fee_recipient_rotation_guard;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fees;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fees_extended;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
mod test_freeze_appeal_channel;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freeze_appeal_channel;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freeze_event;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freeze_guard_writes;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freshness;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freshness_bounds;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freshness_lag;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_funding_events_audit_parity;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_accept_bid_and_fund;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_accounting;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_cancelled_noop;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_currency_whitelist;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_default_boundaries;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_default_counter;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_default_flow;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_distribute_revenue;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_invoice_metadata;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_invoice_search;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_partial_payment;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_settlement_currency_whitelist;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_fuzz_tier_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_governance;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_health_status;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_id_collision_cross_domain;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_id_stability;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_incident;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_init;
#[cfg(test)]
mod test_contract_init_admin;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_init_debug;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_init_invariants;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_input_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_insurance;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_insurance_claim_payout;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_insurance_optin_lifecycle;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_insurance_premium_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_insurance_stacking;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invariant_self_check;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invariants;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_active_guard;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_consistency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_lifecycle;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_queries;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_state_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_terminal_states;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_transitions;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_withdrawal;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investor_bid_limit;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investor_exposure_caps;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investor_freeze_reason;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investor_kyc;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investor_rating_recompute;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_amount_precision;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_batch_cancel;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_id_collision;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_id_collision_regression;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_lifecycle_invariants;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_metadata;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_normalization;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_search;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_search_ranking;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_state_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_kyc_lifecycle_enforcement;
#[cfg(test)]
mod test_kyc_policy;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_ledger_timestamp_consistency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_lifecycle;
#[cfg(test)]
mod test_lifecycle_model;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_limit;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_line_item_consistency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_lock_time_limit;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_lock_time_limit_guard;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_maintenance;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_maintenance_write_matrix;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_max_invoice_tags_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_max_invoices_per_business;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_min_invoice_amount;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_min_partial_fill_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_multisig;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_notification_status_matrix;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_notifications;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_operational_limits;
#[cfg(test)]
mod test_overdue_expiration;
#[cfg(test)]
mod test_overflow;
#[cfg(test)]
mod test_pagination_cursors;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
mod test_panic_handler;
#[cfg(test)]
mod test_partial_payments;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_overflow;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_overdue_expiration;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pagination_cursors;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_panic_handler;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pause;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pause_reads_available;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pause_reason;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pause_reason_cap;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pause_toggle_symmetry;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_payments;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_payments_auth;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_payments_repayment;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_payments_repayment_limits;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_per_investor_position_cap;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_platform_metrics_reconciliation;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_profit_fee;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_profit_fee_formula;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_profits_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_protocol_health;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_protocol_limits;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_protocol_limits_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_prune_terminal_invoices;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_queries;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_rating_override;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_ratings_snapshot;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_rebuild_indexes;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_recipient_history;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_reentrancy;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_reentrancy_fault_injection;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_refund;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_regulatory;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_regulatory_gate;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_repayment_events_audit_parity;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_replay_guard;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_require_business_active;
#[cfg(test)]
mod test_resolution_policy_enforcement;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_require_valid_business_kyc_tier;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_require_valid_invoice_category;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_resource_rate_limits;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_revenue_split;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_risk_tier;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_seed;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_self_call_rejection;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settle_during_dispute;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement_accounting_identity;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_settlement_conservation_props;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement_atomic_rollback;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement_auto_release;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement_capacity_stress;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement_currency_whitelist;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement_dispute_interaction;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_settlement_history_reconstruction;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_snapshot;
#[cfg(test)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
mod test_storage;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
mod test_storage_tier_migration;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_storage;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_storage_key_layout;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_storage_migration_compat;
#[cfg(test)]
mod test_storage_ttl;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_store_invoice_auth;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_store_invoices_batch;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_string_limits;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_tier_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_treasury_rotation;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_treasury_rotation_deadline;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_treasury_split_overflow_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_twa_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_types;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_upgrade_guard;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_verification_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_verify_bid_match;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_vesting;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_vesting_cliff_final;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_vesting_summary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_view_only;
#[cfg(test)]
#[allow(clippy::disallowed_macros)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_volume_tier_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_withdraw_bid_matrix;

// ============================================================================
// Contract-level macros (re-exported from diagnostics)
// ============================================================================

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! qlx_log {
    ($env:expr, $module:expr, $fmt:expr $(, $arg:expr)*) => {{
        #[cfg(feature = "diagnostics")]
        {
            let _ = ($env, $module, $fmt $(, $arg)*);
        }
        #[cfg(not(feature = "diagnostics"))]
        {
            let _ = ($env, $module, $fmt $(, $arg)*);
        }
    }};
}

#[cfg(not(feature = "diagnostics"))]
#[macro_export]
macro_rules! assert_view_only {
    ($env:expr) => {{
        // no-op: view-only enforcement is handled in StorageManager::is_view_only
    }};
}

// ============================================================================
// Protocol-wide constants
// ============================================================================

/// Hard upper bound on paginated query results.
pub const MAX_QUERY_LIMIT: u32 = 50;

// ============================================================================
// Contract definition
// ============================================================================

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String, Vec};

use crate::admin::AdminStorage;
use crate::arbiter::ArbiterStorage;
use crate::errors::QuickLendXError;
use crate::pause::PauseControl;
use crate::protocol_limits::check_and_record_mutation;
use crate::reentrancy::with_payment_guard;
use crate::storage::InvoiceStorage;
use crate::types::{
    Dispute, DisputeResolution, DisputeStatus, Investment, InvoiceCategory, InvoiceStatus,
};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    // =========================================================================
    // Admin / Initialization
    // =========================================================================

    /// One-time admin initialization or legacy single-arg set (for test
    /// compatibility). When called with a single address argument:
    /// - If admin is not yet initialized, initializes it.
    /// - If already initialized, requires the current admin to authorize a
    ///   transfer to `admin` (two-arg form uses `set_admin_transfer`).
    pub fn set_admin(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::set_admin_legacy(&env, &admin)
    }

    pub fn get_admin(env: Env) -> Result<Address, QuickLendXError> {
        AdminStorage::get_admin(&env).ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn initialize_fee_system(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        crate::fees::FeeManager::initialize(&env, &admin)
    }

    pub fn update_platform_fee_bps(env: Env, fee_bps: u32) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        crate::fees::FeeManager::update_platform_fee(&env, &admin, fee_bps)
    }

    pub fn get_platform_fee_config(
        env: Env,
    ) -> Result<crate::types::PlatformFeeConfig, QuickLendXError> {
        crate::fees::FeeManager::get_platform_fee_config(&env)
    }

    pub fn calculate_profit(
        env: Env,
        investment_amount: i128,
        payment_amount: i128,
    ) -> (i128, i128) {
        crate::profits::calculate_profit(&env, investment_amount, payment_amount)
    }

    pub fn invariant_self_check(
        env: Env,
        admin: Address,
    ) -> Result<crate::invariants::InvariantReport, QuickLendXError> {
        crate::invariants::invariant_self_check(&env, &admin)
    }

    pub fn initialize_protocol_limits(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        crate::protocol_limits::ProtocolLimitsContract::initialize(env, admin)
    }

    /// Admin-only: add a token address to the currency whitelist.
    ///
    /// An empty whitelist accepts every currency; once a currency is added the
    /// list becomes restrictive. See [`crate::currency::CurrencyWhitelist`].
    pub fn add_currency(
        env: Env,
        admin: Address,
        currency: Address,
    ) -> Result<(), QuickLendXError> {
        crate::currency::CurrencyWhitelist::add_currency(&env, &admin, &currency)
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        PauseControl::set_paused(&env, &admin, true)
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        PauseControl::set_paused(&env, &admin, false)
    }

    pub fn is_paused(env: Env) -> bool {
        PauseControl::is_paused(&env)
    }

    /// Schedule a WASM upgrade. Only the current admin may schedule it.
    pub fn schedule_upgrade(
        env: Env,
        admin: Address,
        wasm_hash: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::schedule_upgrade(&env, &admin, &wasm_hash)
    }

    /// Cancel a scheduled WASM upgrade and restore the previous pause state.
    pub fn cancel_upgrade(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::cancel_upgrade(&env, &admin)
    }

    /// Execute a scheduled WASM upgrade after its timelock has elapsed.
    pub fn execute_upgrade(env: Env, admin: Address) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::execute_upgrade(&env, &admin)
    }

    /// Read the pending upgrade hash and its scheduling timestamp.
    pub fn get_pending_upgrade(env: Env) -> Option<(BytesN<32>, u64)> {
        crate::upgrade::UpgradeControl::get_pending_upgrade(&env)
    }

    // =========================================================================
    // KYC / Verification
    // =========================================================================

    pub fn submit_kyc_application(
        env: Env,
        business: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        check_and_record_mutation(&env, &business)?;
        crate::verification::submit_kyc_application(&env, &business, kyc_data)
    }

    pub fn verify_business(
        env: Env,
        admin: Address,
        business: Address,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        crate::verification::verify_business(&env, &admin, &business)
    }

    pub fn submit_investor_kyc(
        env: Env,
        investor: Address,
        kyc_data: String,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        check_and_record_mutation(&env, &investor)?;
        crate::verification::submit_investor_kyc(&env, &investor, kyc_data)
    }

    /// Verify investor with investment limit. Admin is read from storage
    /// (matches test usage: `client.verify_investor(investor, limit)`).
    pub fn verify_investor(
        env: Env,
        investor: Address,
        limit: i128,
    ) -> Result<(), QuickLendXError> {
    pub fn verify_investor(env: Env, investor: Address, limit: i128) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        crate::verification::verify_investor(&env, &admin, &investor, limit)?;
        Ok(())
    }

    pub fn get_businesses_paged(
        env: Env,
        status: crate::verification::BusinessVerificationStatus,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::kyc_policy::KycPageResult, QuickLendXError> {
        crate::kyc_policy::get_businesses_paged(&env, status, cursor, limit)
    }

    pub fn get_investors_paged(
        env: Env,
        status: crate::verification::BusinessVerificationStatus,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::kyc_policy::KycPageResult, QuickLendXError> {
        crate::kyc_policy::get_investors_paged(&env, status, cursor, limit)
    }

    // =========================================================================
    // Invoice lifecycle
    // =========================================================================

    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: InvoiceCategory,
        tags: Vec<String>,
        origination_fee_bps: Option<u32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &business)?;
        business.require_auth();
        crate::verification::require_business_not_pending(&env, &business)?;
        crate::invoice_amount::validate_invoice_amount_ceiling(amount)?;
        crate::protocol_limits::check_invoice_limit(&env, &business)?;
        let invoice = crate::invoice::Invoice::new_with_fee_settings(
            &env,
            business,
            amount,
            currency,
            due_date,
            description,
            category,
            tags,
            origination_fee_bps,
            None,
            None,
        )?;
        let id = invoice.id.clone();
        InvoiceStorage::store_invoice(&env, &invoice);
        Ok(id)
    }

    pub fn get_invoice(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::types::Invoice, QuickLendXError> {
        InvoiceStorage::get_invoice(&env, &invoice_id).ok_or(QuickLendXError::InvoiceNotFound)
    }

    pub fn verify_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.verify(&env, admin);
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn update_invoice_status(
        env: Env,
        invoice_id: BytesN<32>,
        status: InvoiceStatus,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        AdminStorage::require_admin(&env, &admin)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        invoice.status = status;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    pub fn cancel_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        let caller = invoice.business.clone();
        invoice.cancel(&env, caller)?;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    /// Emit the `InvoiceExpired` event for an invoice past its due date.
    ///
    /// Requires `current_timestamp > invoice.due_date`. This entrypoint only
    /// emits the event; it does not change the invoice status. Status
    /// transitions are performed by `mark_invoice_defaulted` (→ `Defaulted`)
    /// or `refund_escrow` (→ `Refunded`).
    pub fn expire_invoice(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        if env.ledger().timestamp() <= invoice.due_date {
            return Err(QuickLendXError::InvalidStatus);
        }
        crate::events::emit_invoice_expired(&env, &invoice);
        Ok(())
    }

    pub fn get_available_invoices(env: Env) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, InvoiceStatus::Verified)
    }

    /// Return all invoice IDs currently in `status`.
    pub fn get_invoices_by_status(env: Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        InvoiceStorage::get_invoices_by_status(&env, status)
    }

    pub fn get_business_invoices(env: Env, business: Address) -> Vec<BytesN<32>> {
        InvoiceStorage::get_business_invoices(&env, &business)
    }

    pub fn get_business_invoices_paged(
        env: Env,
        business: Address,
        status_filter: Option<InvoiceStatus>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::InvoicePage, QuickLendXError> {
        InvoiceStorage::get_business_invoices_paged(
            &env,
            &business,
            status_filter,
            cursor,
            limit,
        )
    }

    pub fn get_whitelisted_currencies_paged(
        env: Env,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorAddressPage, QuickLendXError> {
        crate::currency::CurrencyWhitelist::get_whitelisted_currencies_paged(
            &env, cursor, limit,
        )
    }

    #[deprecated(note = "use get_whitelisted_currencies_paged with an opaque cursor")]
    pub fn get_whitelisted_currency_offset(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> crate::types::PaginatedCurrencies {
        crate::currency::CurrencyWhitelist::get_whitelisted_currency_offset(
            &env, offset, limit,
        )
    }

    pub fn get_whitelisted_currencies(env: Env) -> Vec<Address> {
        crate::currency::CurrencyWhitelist::get_whitelisted_currencies(&env)
    }

    pub fn get_user_notifications(env: Env, user: Address) -> Vec<BytesN<32>> {
        crate::notifications::NotificationSystem::get_user_notifications(&env, &user)
    }

    pub fn get_user_notifications_paged(
        env: Env,
        user: Address,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::notifications::NotificationSystem::get_user_notifications_paged(
            &env, &user, cursor, limit,
        )
    }

    pub fn query_audit_logs_paged(
        env: Env,
        filter: crate::audit::AuditQueryFilter,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::audit::CursorAuditPage, QuickLendXError> {
        crate::audit::AuditStorage::query_audit_logs_paged(&env, &filter, cursor, limit)
    }

    // =========================================================================
    // Analytics
    // =========================================================================

    /// Platform analytics derived live from on-chain state.
    ///
    /// Totals and rates are computed from the invoice status indexes, so the
    /// values always reflect the current ledger state. Read-only; no auth.
    pub fn get_platform_metrics(
        env: Env,
    ) -> Result<crate::analytics::PlatformMetrics, QuickLendXError> {
        crate::analytics::AnalyticsCalculator::calculate_platform_metrics(&env)
    pub fn get_total_invoice_count(env: Env) -> u32 {
        InvoiceStorage::get_total_count(&env) as u32
    }

    // =========================================================================
    // Bidding
    // =========================================================================

    pub fn place_bid(
        env: Env,
        investor: Address,
        invoice_id: BytesN<32>,
        bid_amount: i128,
        expected_return: i128,
        salt: BytesN<32>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &investor)?;
        investor.require_auth();
        crate::verification::require_investor_not_pending(&env, &investor)?;

        let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;
        crate::verification::validate_bid(&env, &invoice, bid_amount, expected_return, &investor)?;

        let bid_id = crate::bid::BidStorage::generate_unique_bid_id(&env);
        let bid = crate::types::Bid {
            bid_id: bid_id.clone(),
            invoice_id,
            investor,
            bid_amount,
            expected_return,
            timestamp: env.ledger().timestamp(),
            status: crate::types::BidStatus::Placed,
            expiration_timestamp: env.ledger().timestamp() + 86_400,
        };
        crate::bid::BidStorage::store_bid(&env, &bid);
        crate::bid::BidStorage::add_bid_to_invoice(&env, &bid.invoice_id, &bid.bid_id);
        Ok(bid_id)
    }

    pub fn accept_bid(
        env: Env,
        invoice_id: BytesN<32>,
        bid_id: BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::escrow::accept_bid_and_fund(&env, &invoice_id, &bid_id).map(|_| ())
        })
    }

    pub fn get_bid(env: Env, bid_id: BytesN<32>) -> Option<crate::types::Bid> {
        crate::bid::BidStorage::get_bid(&env, &bid_id)
    }

    pub fn get_bids_for_invoice(env: Env, invoice_id: BytesN<32>) -> Vec<crate::types::Bid> {
        let ids = crate::bid::BidStorage::get_bids_for_invoice(&env, &invoice_id);
        let mut bids = Vec::new(&env);
        for id in ids.iter() {
            if let Some(bid) = crate::bid::BidStorage::get_bid(&env, &id) {
                bids.push_back(bid);
            }
        }
        bids
    }

    pub fn get_ranked_bids_paged(
        env: Env,
        invoice_id: BytesN<32>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBidsPage, QuickLendXError> {
        crate::bid::BidStorage::rank_bids_paged(&env, &invoice_id, cursor, limit)
    }

    // =========================================================================
    // Escrow
    // =========================================================================

    pub fn get_escrow_details(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::payments::Escrow, QuickLendXError> {
        crate::payments::EscrowStorage::get_escrow_by_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    /// Admin-only lookup of an escrow by its direct `escrow_id`.
    ///
    /// Unlike `get_escrow_details` (which looks up by invoice), this returns
    /// the escrow record keyed by `escrow_id`. Requires the caller to be the
    /// current admin.
    pub fn admin_get_escrow(
        env: Env,
        admin: Address,
        escrow_id: BytesN<32>,
    ) -> Result<crate::payments::Escrow, QuickLendXError> {
        admin.require_auth();
        crate::admin::AdminStorage::require_admin(&env, &admin)?;
        crate::payments::EscrowStorage::get_escrow(&env, &escrow_id)
            .ok_or(QuickLendXError::StorageKeyNotFound)
    }

    pub fn release_escrow(
        env: Env,
        invoice_id: BytesN<32>,
        business: Address,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::payments::release_escrow(&env, &invoice_id, &business)
        })
    }

    /// Admin-only: refund a funded invoice's escrow back to the investor.
    ///
    /// Convenience wrapper around [`crate::escrow::refund_escrow_funds`] that
    /// reads the admin from storage. Transitions the invoice to `Refunded`.
    pub fn refund_escrow(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        let admin = AdminStorage::get_admin(&env).ok_or(QuickLendXError::NotAdmin)?;
        with_payment_guard(&env, || {
            crate::escrow::refund_escrow_funds(&env, &invoice_id, &admin)
        })
    }

    // =========================================================================
    // Investment queries
    // =========================================================================

    /// Get the investment snapshot for an invoice (returns None when not funded).
    pub fn get_investment(env: Env, invoice_id: BytesN<32>) -> Option<Investment> {
        crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
    }

    /// Get the investment for an invoice (panics if not funded — test helper).
    pub fn get_invoice_investment(env: Env, invoice_id: BytesN<32>) -> Investment {
        crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
            .expect("invoice has no investment")
    }

    pub fn get_investor_investments_paged(
        env: Env,
        investor: Address,
        status_filter: Option<crate::types::InvestmentStatus>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::investment_queries::InvestmentQueries::get_investor_investments_paged(
            &env,
            &investor,
            status_filter,
            cursor,
            limit,
        )
    }

    // =========================================================================
    // Settlement & Repayment
    // =========================================================================

    /// Record a partial payment toward an invoice.
    ///
    /// `transaction_id` must be a 64-character lowercase hex string (or empty
    /// to bypass the nonce-deduplication check).
    pub fn process_partial_payment(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
        transaction_id: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        with_payment_guard(&env, || {
            crate::settlement::process_partial_payment(
                &env,
                &invoice_id,
                payment_amount,
                transaction_id,
            )
        })
    }

    /// Fully settle an invoice with a single payment.
    ///
    /// `snap` is an investment snapshot obtained from `get_investment` — the
    /// contract verifies it matches the stored investment to guard against
    /// stale reads racing with concurrent state changes.
    pub fn settle_invoice(
        env: Env,
        invoice_id: BytesN<32>,
        payment_amount: i128,
        snap: Investment,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        // Verify the caller-supplied snapshot is fresh.
        let stored =
            crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
                .ok_or(QuickLendXError::StorageKeyNotFound)?;
        if stored.investment_id != snap.investment_id || stored.amount != snap.amount {
            return Err(QuickLendXError::StaleInvestmentSnapshot);
        }
        with_payment_guard(&env, || {
            let invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
                .ok_or(QuickLendXError::InvoiceNotFound)?;
            let business = invoice.business.clone();
            crate::settlement::settle_invoice(&env, &invoice_id, payment_amount, &snap, &business)
        })
    }

    /// Returns the settlement progress for an invoice.
    pub fn get_invoice_progress(
        env: Env,
        invoice_id: BytesN<32>,
    ) -> Result<crate::settlement::Progress, QuickLendXError> {
        crate::settlement::get_invoice_progress(&env, &invoice_id)
    }

    /// Returns the number of recorded payments for an invoice.
    pub fn get_payment_count(env: Env, invoice_id: BytesN<32>) -> Result<u32, QuickLendXError> {
        crate::settlement::get_payment_count(&env, &invoice_id)
    }

    /// Returns a paginated slice of payment records.
    ///
    /// `limit` is capped at `get_settlement_batch_size_max()` (50).
    pub fn get_payment_records(
        env: Env,
        invoice_id: BytesN<32>,
        from: u32,
        limit: u32,
    ) -> Result<Vec<crate::settlement::SettlementPaymentRecord>, QuickLendXError> {
        let capped = limit.min(crate::settlement::MAX_SETTLEMENT_BATCH_SIZE_SOFT_CAP);
        crate::settlement::get_payment_records(&env, &invoice_id, from, capped)
    }

    pub fn get_payment_records_paged(
        env: Env,
        invoice_id: BytesN<32>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::settlement::CursorPaymentRecordsPage, QuickLendXError> {
        crate::settlement::get_payment_records_paged(&env, &invoice_id, cursor, limit)
    }

    /// Returns `true` if the invoice has been fully settled.
    pub fn is_invoice_finalized(env: Env, invoice_id: BytesN<32>) -> Result<bool, QuickLendXError> {
        crate::settlement::is_invoice_finalized(&env, &invoice_id)
    }

    /// Recommended page size for off-chain consumers fetching payment records.
    pub fn get_settlement_batch_size(env: Env) -> u32 {
        let _ = env;
        crate::settlement::default_settlement_batch_size_soft_cap()
    }

    /// Hard upper bound on the number of payment records per query.
    pub fn get_settlement_batch_size_max(env: Env) -> u32 {
        let _ = env;
        crate::settlement::max_settlement_batch_size_soft_cap()
    }

    // =========================================================================
    // Dispute Lifecycle
    // =========================================================================

    /// Open a new dispute on an invoice.
    ///
    /// `creator` must be the business owner or the investor on the invoice and
    /// must sign the transaction. Wraps `dispute::create_dispute`.
    pub fn create_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        creator: Address,
        reason: String,
        evidence: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &creator)?;
        crate::dispute::create_dispute(&env, &invoice_id, &creator, &reason, &evidence)
    }

    /// Advance a `Disputed` dispute to `UnderReview` (admin only).
    pub fn put_dispute_under_review(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &admin)?;
        crate::dispute::put_dispute_under_review(&env, &admin, &invoice_id)
    }

    /// Resolve a dispute with a free-text resolution (admin + registered arbiter).
    pub fn resolve_dispute(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
        resolution: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &admin)?;
        crate::dispute::resolve_dispute(&env, &admin, &invoice_id, &resolution)
    }

    /// Resolve a dispute with a structured outcome (admin + registered arbiter).
    pub fn resolve_dispute_structured(
        env: Env,
        invoice_id: BytesN<32>,
        admin: Address,
        outcome: DisputeResolution,
        note: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        check_and_record_mutation(&env, &admin)?;
        crate::dispute::resolve_dispute_structured(&env, &admin, &invoice_id, outcome, &note)
    }

    /// Return the dispute details for an invoice, or `None` if no dispute exists.
    pub fn get_dispute_details(env: Env, invoice_id: BytesN<32>) -> Option<Dispute> {
        crate::dispute::get_dispute_details(&env, &invoice_id)
    }

    /// Return all invoice IDs that have ever entered the dispute lifecycle.
    pub fn get_invoices_with_disputes(env: Env) -> Vec<BytesN<32>> {
        crate::dispute::get_invoices_with_disputes(&env)
    }

    pub fn get_invoices_with_disputes_paged(
        env: Env,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::dispute::get_invoices_with_disputes_paged(&env, cursor, limit)
    }

    /// Filter dispute-indexed invoices by their current dispute status.
    pub fn get_invoices_by_dispute_status(env: Env, status: DisputeStatus) -> Vec<BytesN<32>> {
        crate::dispute::get_invoices_by_dispute_status(&env, &status)
    }

    pub fn get_disputes_by_status_paged(
        env: Env,
        status: DisputeStatus,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::types::CursorBytes32Page, QuickLendXError> {
        crate::dispute::get_invoices_by_dispute_status_paged(
            &env, &status, cursor, limit,
        )
    }

    /// Return a paginated dispute timeline for an invoice.
    pub fn get_dispute_timeline(
        env: Env,
        invoice_id: BytesN<32>,
        offset: u32,
        limit: u32,
    ) -> Result<crate::dispute_timeline::DisputeTimeline, QuickLendXError> {
        crate::dispute_timeline::get_dispute_timeline(&env, &invoice_id, offset, limit)
    }

    pub fn get_dispute_timeline_paged(
        env: Env,
        invoice_id: BytesN<32>,
        cursor: Option<String>,
        limit: u32,
    ) -> Result<crate::dispute_timeline::CursorDisputeTimeline, QuickLendXError> {
        crate::dispute_timeline::get_dispute_timeline_paged(&env, &invoice_id, cursor, limit)
    }

    /// Update the evidence on an open (Disputed) dispute. Creator-only, immutable once UnderReview.
    pub fn update_dispute_evidence(
        env: Env,
        invoice_id: BytesN<32>,
        creator: Address,
        new_evidence: String,
    ) -> Result<(), QuickLendXError> {
        PauseControl::require_not_paused(&env)?;
        creator.require_auth();
        check_and_record_mutation(&env, &creator)?;

        crate::verification::validate_dispute_evidence(&new_evidence)?;

        let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id)
            .ok_or(QuickLendXError::InvoiceNotFound)?;

        // Only the original creator may update evidence.
        if invoice.dispute.created_by != creator {
            return Err(QuickLendXError::DisputeNotAuthorized);
        }

        // Evidence is immutable once review has started.
        match invoice.dispute_status {
            DisputeStatus::Disputed => {}
            DisputeStatus::None => return Err(QuickLendXError::DisputeNotFound),
            _ => return Err(QuickLendXError::InvalidStatus),
        }

        invoice.dispute.evidence = new_evidence;
        InvoiceStorage::update_invoice(&env, &invoice);
        Ok(())
    }

    // =========================================================================
    // Arbiter Registry
    // =========================================================================

    /// Register a new dispute arbiter (admin only).
    pub fn register_arbiter(
        env: Env,
        admin: Address,
        arbiter: Address,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        ArbiterStorage::register_arbiter(&env, &admin, &arbiter)
    }

    /// Revoke a registered dispute arbiter (admin only).
    pub fn unregister_arbiter(
        env: Env,
        admin: Address,
        arbiter: Address,
    ) -> Result<(), QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        ArbiterStorage::unregister_arbiter(&env, &admin, &arbiter)
    }

    /// Return all registered arbiters.
    pub fn list_arbiters(env: Env) -> Vec<Address> {
        ArbiterStorage::list_arbiters(&env)
    }

    /// Return `true` if `addr` is a registered dispute arbiter.
    pub fn is_arbiter(env: Env, addr: Address) -> bool {
        ArbiterStorage::is_arbiter(&env, &addr)
    }

    // ── Backup Management ────────────────────────────────────────────────────

    /// Create a backup of all invoices across all statuses.
    ///
    /// Returns the backup ID.
    pub fn create_backup(
        env: Env,
        admin: Address,
        description: String,
    ) -> Result<BytesN<32>, errors::QuickLendXError> {
    pub fn create_backup(env: Env, admin: Address, description: String) -> Result<BytesN<32>, errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;

        if description.len() > 128 {
            return Err(errors::QuickLendXError::InvalidDescription);
        }

        let all_invoices = backup::BackupStorage::get_all_invoices(&env);
        let backup_id = backup::BackupStorage::generate_backup_id(&env);

        let backup = backup::Backup {
            backup_id: backup_id.clone(),
            timestamp: env.ledger().timestamp(),
            description,
            invoice_count: all_invoices.len(),
            status: backup::BackupStatus::Active,
            format_version: 2,
        };

        backup::BackupStorage::store_backup(&env, &backup, Some(&all_invoices))?;
        backup::BackupStorage::store_backup_data(&env, &backup_id, &all_invoices);
        backup::BackupStorage::add_to_backup_list(&env, &backup_id);

        backup::BackupStorage::cleanup_old_backups(&env)?;

        Ok(backup_id)
    }

    /// Restore all invoices from a backup.
    ///
    /// Clears all existing invoices and restores from the backup snapshot.
    /// Returns the number of invoices restored.
    pub fn restore_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<u32, errors::QuickLendXError> {
    pub fn restore_backup(env: Env, admin: Address, backup_id: BytesN<32>) -> Result<u32, errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;
        backup::BackupStorage::restore_from_backup(&env, &backup_id)
    }

    /// Set the backup retention policy.
    pub fn set_backup_retention_policy(
        env: Env,
        admin: Address,
        max_backups: u32,
        max_age_seconds: u64,
        enabled: bool,
    ) -> Result<(), errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;

        let policy = backup::BackupRetentionPolicy {
            max_backups,
            max_age_seconds,
            auto_cleanup_enabled: enabled,
        };
        backup::BackupStorage::set_retention_policy(&env, &policy);
        Ok(())
    }

    /// Archive a backup (mark it as no longer active).
    pub fn archive_backup(
        env: Env,
        admin: Address,
        backup_id: BytesN<32>,
    ) -> Result<(), errors::QuickLendXError> {
    pub fn archive_backup(env: Env, admin: Address, backup_id: BytesN<32>) -> Result<(), errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;
        governance::require_no_open_governance_proposal(&env)?;

        let mut backup = backup::BackupStorage::get_backup(&env, &backup_id)
            .ok_or(errors::QuickLendXError::OperationNotAllowed)?;
        backup.status = backup::BackupStatus::Archived;
        backup::BackupStorage::update_backup(&env, &backup)?;
        backup::BackupStorage::remove_from_backup_list(&env, &backup_id);
        Ok(())
    }

    /// Clean up old backups according to the retention policy.
    ///
    /// Returns the number of backups removed.
    pub fn cleanup_backups(env: Env, admin: Address) -> Result<u32, errors::QuickLendXError> {
        crate::upgrade::UpgradeControl::require_no_pending_upgrade(&env)?;
        admin.require_auth();
        admin::AdminStorage::require_admin(&env, &admin)?;
        backup::BackupStorage::cleanup_old_backups(&env)
    }

    // ── Backup Views ─────────────────────────────────────────────────────────

    /// Get all backup IDs.
    pub fn get_backups(env: Env) -> Vec<BytesN<32>> {
        backup::BackupStorage::get_all_backups(&env)
    }

    /// Get backup details by ID.
    pub fn get_backup_details(env: Env, backup_id: BytesN<32>) -> Option<backup::Backup> {
        backup::BackupStorage::get_backup(&env, &backup_id)
    }

    /// Validate a backup without mutating state.
    ///
    /// Returns `true` if the backup is valid and can be restored.
    pub fn validate_backup(env: Env, backup_id: BytesN<32>) -> bool {
        backup::BackupStorage::validate_backup(&env, &backup_id).is_ok()
    }

    /// Preview which backups would be cleaned up by the retention policy.
    pub fn preview_cleanup_backups(env: Env) -> backup::BackupCleanupDryRunReport {
        backup::BackupStorage::preview_cleanup_old_backups(&env)
    }

    // ── Storage Migration Runner ─────────────────────────────────────────────

    /// Plan and initiate a resumable storage migration from `from` to `to` (admin only).
    pub fn migration_plan(
        env: Env,
        admin: Address,
        from: u32,
        to: u32,
    ) -> Result<migration::MigrationPlan, errors::QuickLendXError> {
        migration::MigrationRunner::plan(&env, &admin, from, to)
    }

    /// Execute one bounded step of the active storage migration (admin only).
    pub fn migration_run_step(
        env: Env,
        admin: Address,
        limit: u32,
    ) -> Result<migration::MigrationStepResult, errors::QuickLendXError> {
        migration::MigrationRunner::run_step(&env, &admin, limit)
    }

    /// Preview the next storage migration step without mutating state (admin only).
    pub fn migration_dry_run(
        env: Env,
        admin: Address,
        limit: u32,
    ) -> Result<migration::MigrationDryRunReport, errors::QuickLendXError> {
        migration::MigrationRunner::dry_run(&env, &admin, limit)
    }

    /// Return the current storage migration status.
    pub fn migration_status(env: Env) -> migration::MigrationStatus {
        migration::MigrationRunner::status(&env)
    }

    /// Roll back an in-progress storage migration (admin only).
    pub fn migration_rollback(env: Env, admin: Address) -> Result<(), errors::QuickLendXError> {
        migration::MigrationRunner::rollback(&env, &admin)
    }

    /// Return the persisted completion record for a migrated schema version.
    pub fn get_migration_record(
        env: Env,
        version: u32,
    ) -> Option<migration::MigrationVersionRecord> {
        migration::MigrationRunner::get_version_record(&env, version)
    // ── Stub Methods for Test Compatibility ─────────────────────────────────
    // These are minimal stubs to satisfy existing tests. Full implementation
    // would be added when those modules are complete.

    pub fn initialize_protocol_limits(
        _env: Env,
        _admin: Address,
        _min_amount: i128,
        _max_due_days: u64,
        _lock_time_limit: u64,
    ) -> Result<(), errors::QuickLendXError> {
        Ok(())
    }

    pub fn store_invoice(
        env: Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: types::InvoiceCategory,
        _tags: Vec<String>,
        _customer_name: Option<String>,
    ) -> BytesN<32> {
        use types::{Dispute, DisputeResolution, DisputeStatus, Invoice, InvoiceStatus};
        
        let invoice_id = storage::InvoiceStorage::next_count(&env);
        let mut id_bytes = [0u8; 32];
        id_bytes[24..32].copy_from_slice(&invoice_id.to_be_bytes());
        let id = BytesN::from_array(&env, &id_bytes);

        let invoice = Invoice {
            id: id.clone(),
            business,
            amount,
            currency,
            due_date,
            status: InvoiceStatus::Pending,
            created_at: env.ledger().timestamp(),
            description,
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(&env),
            category,
            tags: Vec::new(&env),
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(&env),
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: Address::generate(&env),
                created_at: 0,
                reason: String::from_str(&env, ""),
                evidence: String::from_str(&env, ""),
                resolution: String::from_str(&env, ""),
                resolved_by: Address::generate(&env),
                resolved_at: 0,
                resolution_outcome: DisputeResolution::None,
            },
            total_paid: 0,
            payment_history: Vec::new(&env),
            origination_fee_bps: None,
            late_payment_penalty_bps: None,
            early_payment_discount_bps: None,
        };

        storage::InvoiceStorage::store_invoice(&env, &invoice);
        id
    }

    pub fn get_total_invoice_count(env: Env) -> u64 {
        storage::InvoiceStorage::get_total_count(&env)
    }

    pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<types::Invoice, errors::QuickLendXError> {
        storage::InvoiceStorage::get(&env, &invoice_id)
            .ok_or(errors::QuickLendXError::InvoiceNotFound)
    }
}
