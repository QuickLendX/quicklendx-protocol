//! Declarations for the crate's `src/test_*.rs` unit-test modules.
//!
//! Keeping this list here instead of in `src/lib.rs` keeps the crate root
//! small and avoids merge conflicts in every test PR. Test files are not
//! moved: each declaration points at its existing file with `#[path]`.
//!
//! Run `scripts/check-test-modules.sh` to list test files that are missing
//! a declaration here.

// These modules used to be direct children of the crate root, and many of them
// reach crate items with `use super::*;` or `super::<module>::...`. Import the
// crate root here so `super` inside a test module still resolves to it.
#[allow(unused_imports)]
use crate::*;

#[cfg(feature = "legacy-tests")]
#[path = "../test_accept_bid_instruction_budget.rs"]
mod test_accept_bid_instruction_budget;

#[cfg(feature = "legacy-tests")]
#[path = "../test_accept_bid_race.rs"]
mod test_accept_bid_race;

#[allow(clippy::disallowed_macros)]
#[path = "../test_address_summary.rs"]
mod test_address_summary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_admin.rs"]
mod test_admin;

#[cfg(feature = "legacy-tests")]
#[path = "../test_admin_events_audit_parity.rs"]
mod test_admin_events_audit_parity;

#[cfg(feature = "legacy-tests")]
#[path = "../test_admin_handover.rs"]
mod test_admin_handover;

#[cfg(feature = "legacy-tests")]
#[path = "../test_admin_lookalike.rs"]
mod test_admin_lookalike;

#[cfg(feature = "legacy-tests")]
#[path = "../test_admin_simple.rs"]
mod test_admin_simple;

#[cfg(feature = "legacy-tests")]
#[path = "../test_admin_standalone.rs"]
mod test_admin_standalone;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_admin_two_step.rs"]
mod test_admin_two_step;

#[cfg(feature = "legacy-tests")]
#[path = "../test_analytics_consistency.rs"]
mod test_analytics_consistency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_audit.rs"]
mod test_audit;

#[cfg(feature = "legacy-tests")]
#[path = "../test_audit_config.rs"]
mod test_audit_config;

#[cfg(feature = "legacy-tests")]
#[path = "../test_audit_stats_regression.rs"]
mod test_audit_stats_regression;

#[cfg(feature = "legacy-tests")]
#[path = "../test_authorization.rs"]
mod test_authorization;

#[path = "../test_auto_resolution_boundary.rs"]
mod test_auto_resolution_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_backfill_guard.rs"]
mod test_backfill_guard;

#[cfg(feature = "legacy-tests")]
#[path = "../test_backpressure_shedding.rs"]
mod test_backpressure_shedding;

#[cfg(feature = "legacy-tests")]
#[path = "../test_backup.rs"]
mod test_backup;

#[cfg(feature = "legacy-tests")]
#[path = "../test_backup_restore_reindex.rs"]
mod test_backup_restore_reindex;

#[cfg(feature = "legacy-tests")]
#[path = "../test_backup_retention_enforcement.rs"]
mod test_backup_retention_enforcement;

#[cfg(feature = "legacy-tests")]
#[path = "../test_backup_safety.rs"]
mod test_backup_safety;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid.rs"]
mod test_bid;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_acceptance_replay.rs"]
mod test_bid_acceptance_replay;

#[path = "../test_bid_active_counts_history.rs"]
mod test_bid_active_counts_history;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_atomic_rollback.rs"]
mod test_bid_atomic_rollback;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_audit_parity.rs"]
mod test_bid_audit_parity;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_cancel_accept_race.rs"]
mod test_bid_cancel_accept_race;

#[cfg(feature = "legacy-tests")]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_bid_capacity_stress.rs"]
mod test_bid_capacity_stress;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_compare_order_props.rs"]
mod test_bid_compare_order_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_concurrency.rs"]
mod test_bid_concurrency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_exclusivity.rs"]
mod test_bid_exclusivity;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_expiry_boundary.rs"]
mod test_bid_expiry_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_expiry_grace.rs"]
mod test_bid_expiry_grace;

#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_bid_match_helper.rs"]
mod test_bid_match_helper;

#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_bid_ranking.rs"]
mod test_bid_ranking;

#[cfg(feature = "legacy-tests")]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_bid_ranking_determinism.rs"]
mod test_bid_ranking_determinism;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_ranking_tiebreaker.rs"]
mod test_bid_ranking_tiebreaker;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_state_matrix.rs"]
mod test_bid_state_matrix;

#[allow(clippy::disallowed_macros)]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_bid_transition_matrix.rs"]
mod test_bid_transition_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_ttl.rs"]
mod test_bid_ttl;

#[cfg(feature = "legacy-tests")]
#[path = "../test_bid_validation.rs"]
mod test_bid_validation;

#[cfg(feature = "legacy-tests")]
#[path = "../test_business_freeze_reason.rs"]
mod test_business_freeze_reason;

#[cfg(feature = "legacy-tests")]
#[path = "../test_business_invoices_paged_ordering.rs"]
mod test_business_invoices_paged_ordering;

#[cfg(feature = "legacy-tests")]
#[path = "../test_business_kyc.rs"]
mod test_business_kyc;

#[cfg(feature = "legacy-tests")]
#[path = "../test_cancel_invoice_matrix.rs"]
mod test_cancel_invoice_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_cancel_refund.rs"]
mod test_cancel_refund;

#[cfg(feature = "legacy-tests")]
#[path = "../test_cannot_withdraw_more_than_deposited.rs"]
mod test_cannot_withdraw_more_than_deposited;

#[cfg(feature = "legacy-tests")]
#[path = "../test_category_breakdown.rs"]
mod test_category_breakdown;

#[cfg(feature = "legacy-tests")]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_cleanup_pagination.rs"]
mod test_cleanup_pagination;

#[cfg(feature = "legacy-tests")]
#[path = "../test_clock_rollover.rs"]
mod test_clock_rollover;

#[cfg(feature = "legacy-tests")]
#[path = "../test_compute_yield_props.rs"]
mod test_compute_yield_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_concurrent_default_overlap.rs"]
mod test_concurrent_default_overlap;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_config_bounds_matrix.rs"]
mod test_config_bounds_matrix;

#[path = "../test_contract_init_admin.rs"]
mod test_contract_init_admin;

#[cfg(feature = "legacy-tests")]
#[path = "../test_cross_module_consistency.rs"]
mod test_cross_module_consistency;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_currency.rs"]
mod test_currency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_currency_batch.rs"]
mod test_currency_batch;

#[cfg(feature = "legacy-tests")]
#[path = "../test_currency_match_funding.rs"]
mod test_currency_match_funding;

#[cfg(feature = "legacy-tests")]
#[path = "../test_currency_precision.rs"]
mod test_currency_precision;

#[cfg(feature = "legacy-tests")]
#[path = "../test_default.rs"]
mod test_default;

#[cfg(feature = "legacy-tests")]
#[path = "../test_default_finality.rs"]
mod test_default_finality;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_default_finality_matrix.rs"]
mod test_default_finality_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_default_grace_boundary.rs"]
mod test_default_grace_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_diagnostics.rs"]
mod test_diagnostics;

#[path = "../test_dispute.rs"]
mod test_dispute;

#[path = "../test_dispute_arbiter.rs"]
mod test_dispute_arbiter;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_dispute_event_invariant.rs"]
mod test_dispute_event_invariant;

#[cfg(feature = "legacy-tests")]
#[path = "../test_dispute_evidence_identity.rs"]
mod test_dispute_evidence_identity;

#[cfg(feature = "legacy-tests")]
#[path = "../test_dispute_history_guard.rs"]
mod test_dispute_history_guard;

#[cfg(feature = "legacy-tests")]
#[path = "../test_dispute_refund_flow.rs"]
mod test_dispute_refund_flow;

#[cfg(feature = "legacy-tests")]
#[path = "../test_dispute_settlement.rs"]
mod test_dispute_settlement;

#[cfg(feature = "legacy-tests")]
#[path = "../test_dispute_state_matrix.rs"]
mod test_dispute_state_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_dispute_time_limit.rs"]
mod test_dispute_time_limit;

#[path = "../test_dispute_timeline.rs"]
mod test_dispute_timeline;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_dispute_timeline_props.rs"]
mod test_dispute_timeline_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_due_date_boundaries.rs"]
mod test_due_date_boundaries;

#[cfg(feature = "legacy-tests")]
#[path = "../test_due_date_guard.rs"]
mod test_due_date_guard;

#[cfg(feature = "legacy-tests")]
#[path = "../test_dust_transfer.rs"]
mod test_dust_transfer;

#[cfg(feature = "legacy-tests")]
#[path = "../test_early_payment_discount_bps.rs"]
mod test_early_payment_discount_bps;

#[cfg(feature = "legacy-tests")]
#[path = "../test_emergency.rs"]
mod test_emergency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_emergency_escrow_protection.rs"]
mod test_emergency_escrow_protection;

#[cfg(feature = "legacy-tests")]
#[path = "../test_emergency_withdraw.rs"]
mod test_emergency_withdraw;

#[cfg(feature = "legacy-tests")]
#[path = "../test_emergency_withdraw_props.rs"]
mod test_emergency_withdraw_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_entrypoint_auth_matrix.rs"]
mod test_entrypoint_auth_matrix;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_error_code_stability.rs"]
mod test_error_code_stability;

#[cfg(feature = "legacy-tests")]
#[path = "../test_errors.rs"]
mod test_errors;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow.rs"]
mod test_escrow;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_consistency.rs"]
mod test_escrow_consistency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_early_release.rs"]
mod test_escrow_early_release;

#[allow(clippy::disallowed_macros)]
#[path = "../test_escrow_entrypoints.rs"]
mod test_escrow_entrypoints;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_event_completeness.rs"]
mod test_escrow_event_completeness;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_expiry_extend.rs"]
mod test_escrow_expiry_extend;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_invariant_model.rs"]
mod test_escrow_invariant_model;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_mutual_exclusion.rs"]
mod test_escrow_mutual_exclusion;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_refund.rs"]
mod test_escrow_refund;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_refund_after_expiry.rs"]
mod test_escrow_refund_after_expiry;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_refund_hardened.rs"]
mod test_escrow_refund_hardened;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_settle_refund_race.rs"]
mod test_escrow_settle_refund_race;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_state_machine.rs"]
mod test_escrow_state_machine;

#[cfg(feature = "legacy-tests")]
#[path = "../test_escrow_time_limit.rs"]
mod test_escrow_time_limit;

#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[allow(clippy::disallowed_macros)]
#[path = "../test_escrow_uniqueness.rs"]
mod test_escrow_uniqueness;

#[cfg(feature = "legacy-tests")]
#[path = "../test_events.rs"]
mod test_events;

#[cfg(feature = "legacy-tests")]
#[path = "../test_evidence_hash_format.rs"]
mod test_evidence_hash_format;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_evidence_kind_guard_matrix.rs"]
mod test_evidence_kind_guard_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_evidence_size_cap.rs"]
mod test_evidence_size_cap;

#[cfg(feature = "legacy-tests")]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_expired_bids_cleanup.rs"]
mod test_expired_bids_cleanup;

#[cfg(feature = "legacy-tests")]
#[path = "../test_expired_escrow.rs"]
mod test_expired_escrow;

#[path = "../test_fee_admin_auth.rs"]
mod test_fee_admin_auth;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fee_analytics_boundaries.rs"]
mod test_fee_analytics_boundaries;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fee_recipient_rotation_guard.rs"]
mod test_fee_recipient_rotation_guard;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fees.rs"]
mod test_fees;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fees_extended.rs"]
mod test_fees_extended;

#[allow(clippy::disallowed_macros)]
#[path = "../test_footprint_limits.rs"]
mod test_footprint_limits;

#[allow(clippy::disallowed_macros)]
#[path = "../test_freeze_appeal_channel.rs"]
mod test_freeze_appeal_channel;

#[cfg(feature = "legacy-tests")]
#[path = "../test_freeze_event.rs"]
mod test_freeze_event;

#[cfg(feature = "legacy-tests")]
#[path = "../test_freeze_guard_writes.rs"]
mod test_freeze_guard_writes;

#[cfg(feature = "legacy-tests")]
#[path = "../test_freshness.rs"]
mod test_freshness;

#[cfg(feature = "legacy-tests")]
#[path = "../test_freshness_bounds.rs"]
mod test_freshness_bounds;

#[cfg(feature = "legacy-tests")]
#[path = "../test_freshness_lag.rs"]
mod test_freshness_lag;

#[cfg(feature = "legacy-tests")]
#[path = "../test_funding_events_audit_parity.rs"]
mod test_funding_events_audit_parity;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz.rs"]
mod test_fuzz;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_fuzz_accept_bid_and_fund.rs"]
mod test_fuzz_accept_bid_and_fund;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_accounting.rs"]
mod test_fuzz_accounting;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_cancelled_noop.rs"]
mod test_fuzz_cancelled_noop;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_currency_whitelist.rs"]
mod test_fuzz_currency_whitelist;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_default_boundaries.rs"]
mod test_fuzz_default_boundaries;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_default_counter.rs"]
mod test_fuzz_default_counter;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_default_flow.rs"]
mod test_fuzz_default_flow;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_distribute_revenue.rs"]
mod test_fuzz_distribute_revenue;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_invoice_metadata.rs"]
mod test_fuzz_invoice_metadata;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_invoice_search.rs"]
mod test_fuzz_invoice_search;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_fuzz_partial_payment.rs"]
mod test_fuzz_partial_payment;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_settlement_currency_whitelist.rs"]
mod test_fuzz_settlement_currency_whitelist;

#[cfg(feature = "legacy-tests")]
#[path = "../test_fuzz_tier_boundary.rs"]
mod test_fuzz_tier_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_governance.rs"]
mod test_governance;

#[cfg(feature = "legacy-tests")]
#[path = "../test_health_status.rs"]
mod test_health_status;

#[cfg(feature = "legacy-tests")]
#[path = "../test_id_collision_cross_domain.rs"]
mod test_id_collision_cross_domain;

#[cfg(feature = "legacy-tests")]
#[path = "../test_id_stability.rs"]
mod test_id_stability;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_idempotency.rs"]
mod test_idempotency;

#[path = "../test_incident.rs"]
mod test_incident;

#[cfg(feature = "legacy-tests")]
#[path = "../test_init.rs"]
mod test_init;

#[cfg(feature = "legacy-tests")]
#[path = "../test_init_debug.rs"]
mod test_init_debug;

#[cfg(feature = "legacy-tests")]
#[path = "../test_init_invariants.rs"]
mod test_init_invariants;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_input_matrix.rs"]
mod test_input_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_insurance.rs"]
mod test_insurance;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_insurance_claim_payout.rs"]
mod test_insurance_claim_payout;

#[cfg(feature = "legacy-tests")]
#[path = "../test_insurance_optin_lifecycle.rs"]
mod test_insurance_optin_lifecycle;

#[cfg(feature = "legacy-tests")]
#[path = "../test_insurance_premium_props.rs"]
mod test_insurance_premium_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_insurance_stacking.rs"]
mod test_insurance_stacking;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_invariant_self_check.rs"]
mod test_invariant_self_check;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invariants.rs"]
mod test_invariants;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investment_active_guard.rs"]
mod test_investment_active_guard;

#[cfg(feature = "legacy-tests")]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_investment_consistency.rs"]
mod test_investment_consistency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investment_lifecycle.rs"]
mod test_investment_lifecycle;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investment_queries.rs"]
mod test_investment_queries;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investment_state_matrix.rs"]
mod test_investment_state_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investment_terminal_states.rs"]
mod test_investment_terminal_states;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investment_transitions.rs"]
mod test_investment_transitions;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investment_withdrawal.rs"]
mod test_investment_withdrawal;

#[cfg(feature = "legacy-tests")]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_investor_bid_limit.rs"]
mod test_investor_bid_limit;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investor_exposure_caps.rs"]
mod test_investor_exposure_caps;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investor_freeze_reason.rs"]
mod test_investor_freeze_reason;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investor_kyc.rs"]
mod test_investor_kyc;

#[cfg(feature = "legacy-tests")]
#[path = "../test_investor_rating_recompute.rs"]
mod test_investor_rating_recompute;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_invoice.rs"]
mod test_invoice;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_amount_precision.rs"]
mod test_invoice_amount_precision;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_batch_cancel.rs"]
mod test_invoice_batch_cancel;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_id_collision.rs"]
mod test_invoice_id_collision;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_id_collision_regression.rs"]
mod test_invoice_id_collision_regression;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_lifecycle_invariants.rs"]
mod test_invoice_lifecycle_invariants;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_metadata.rs"]
mod test_invoice_metadata;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_normalization.rs"]
mod test_invoice_normalization;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_search.rs"]
mod test_invoice_search;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_search_ranking.rs"]
mod test_invoice_search_ranking;

#[cfg(feature = "legacy-tests")]
#[path = "../test_invoice_state_matrix.rs"]
mod test_invoice_state_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_kyc_lifecycle_enforcement.rs"]
mod test_kyc_lifecycle_enforcement;

#[path = "../test_kyc_policy.rs"]
mod test_kyc_policy;

#[cfg(feature = "legacy-tests")]
#[path = "../test_ledger_timestamp_consistency.rs"]
mod test_ledger_timestamp_consistency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_lifecycle.rs"]
mod test_lifecycle;

#[path = "../test_lifecycle_model.rs"]
mod test_lifecycle_model;

#[cfg(feature = "legacy-tests")]
#[path = "../test_limit.rs"]
mod test_limit;

#[cfg(feature = "legacy-tests")]
#[path = "../test_line_item_consistency.rs"]
mod test_line_item_consistency;

#[cfg(feature = "legacy-tests")]
#[path = "../test_lock_time_limit.rs"]
mod test_lock_time_limit;

#[cfg(feature = "legacy-tests")]
#[path = "../test_lock_time_limit_guard.rs"]
mod test_lock_time_limit_guard;

#[cfg(feature = "legacy-tests")]
#[path = "../test_maintenance.rs"]
mod test_maintenance;

#[cfg(feature = "legacy-tests")]
#[path = "../test_maintenance_write_matrix.rs"]
mod test_maintenance_write_matrix;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_max_invoice_tags_boundary.rs"]
mod test_max_invoice_tags_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_max_invoices_per_business.rs"]
mod test_max_invoices_per_business;

#[cfg(feature = "legacy-tests")]
#[path = "../test_min_invoice_amount.rs"]
mod test_min_invoice_amount;

#[cfg(feature = "legacy-tests")]
#[path = "../test_min_partial_fill_boundary.rs"]
mod test_min_partial_fill_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_multisig.rs"]
mod test_multisig;

#[cfg(feature = "legacy-tests")]
#[path = "../test_notification_status_matrix.rs"]
mod test_notification_status_matrix;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_notifications.rs"]
mod test_notifications;

#[cfg(feature = "legacy-tests")]
#[path = "../test_operational_limits.rs"]
mod test_operational_limits;

#[path = "../test_overdue_expiration.rs"]
mod test_overdue_expiration;

#[path = "../test_overdue_scan_resilience.rs"]
mod test_overdue_scan_resilience;

#[allow(clippy::disallowed_macros)]
#[path = "../test_overflow.rs"]
mod test_overflow;

#[cfg(feature = "legacy-tests")]
#[path = "../test_pagination_cursors.rs"]
mod test_pagination_cursors;

#[allow(clippy::disallowed_macros)]
#[path = "../test_panic_handler.rs"]
mod test_panic_handler;

#[path = "../test_partial_payments.rs"]
mod test_partial_payments;

#[cfg(feature = "legacy-tests")]
#[path = "../test_pause.rs"]
mod test_pause;

#[cfg(feature = "legacy-tests")]
#[path = "../test_pause_reads_available.rs"]
mod test_pause_reads_available;

#[cfg(feature = "legacy-tests")]
#[path = "../test_pause_reason.rs"]
mod test_pause_reason;

#[cfg(feature = "legacy-tests")]
#[path = "../test_pause_reason_cap.rs"]
mod test_pause_reason_cap;

#[cfg(feature = "legacy-tests")]
#[path = "../test_pause_toggle_symmetry.rs"]
mod test_pause_toggle_symmetry;

#[cfg(feature = "legacy-tests")]
#[path = "../test_payment_history.rs"]
mod test_payment_history;

#[cfg(feature = "legacy-tests")]
#[path = "../test_payment_token_policy.rs"]
mod test_payment_token_policy;

#[cfg(feature = "legacy-tests")]
#[path = "../test_payments.rs"]
mod test_payments;

#[cfg(feature = "legacy-tests")]
#[path = "../test_payments_auth.rs"]
mod test_payments_auth;

#[cfg(feature = "legacy-tests")]
#[path = "../test_payments_repayment.rs"]
mod test_payments_repayment;

#[cfg(feature = "legacy-tests")]
#[path = "../test_payments_repayment_limits.rs"]
mod test_payments_repayment_limits;

#[cfg(feature = "legacy-tests")]
#[path = "../test_per_investor_position_cap.rs"]
mod test_per_investor_position_cap;

#[path = "../test_platform_fee_ownership.rs"]
mod test_platform_fee_ownership;

#[cfg(feature = "legacy-tests")]
#[path = "../test_platform_metrics_reconciliation.rs"]
mod test_platform_metrics_reconciliation;

#[cfg(feature = "legacy-tests")]
#[path = "../test_profit_fee.rs"]
mod test_profit_fee;

#[cfg(feature = "legacy-tests")]
#[path = "../test_profit_fee_formula.rs"]
mod test_profit_fee_formula;

#[cfg(feature = "legacy-tests")]
#[path = "../test_profits_props.rs"]
mod test_profits_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_protocol_health.rs"]
mod test_protocol_health;

#[cfg(feature = "legacy-tests")]
#[path = "../test_protocol_limits.rs"]
mod test_protocol_limits;

#[cfg(feature = "legacy-tests")]
#[path = "../test_protocol_limits_boundary.rs"]
mod test_protocol_limits_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_prune_terminal_invoices.rs"]
mod test_prune_terminal_invoices;

#[cfg(feature = "legacy-tests")]
#[path = "../test_queries.rs"]
mod test_queries;

#[path = "../test_rating_auth.rs"]
mod test_rating_auth;

#[cfg(feature = "legacy-tests")]
#[path = "../test_rating_override.rs"]
mod test_rating_override;

#[cfg(feature = "legacy-tests")]
#[path = "../test_ratings_snapshot.rs"]
mod test_ratings_snapshot;

#[cfg(feature = "legacy-tests")]
#[path = "../test_rebuild_indexes.rs"]
mod test_rebuild_indexes;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_recipient_history.rs"]
mod test_recipient_history;

#[cfg(feature = "legacy-tests")]
#[path = "../test_reentrancy.rs"]
mod test_reentrancy;

#[cfg(feature = "legacy-tests")]
#[path = "../test_reentrancy_fault_injection.rs"]
mod test_reentrancy_fault_injection;

#[cfg(feature = "legacy-tests")]
#[path = "../test_refund.rs"]
mod test_refund;

#[cfg(feature = "legacy-tests")]
#[path = "../test_regulatory.rs"]
mod test_regulatory;

#[cfg(feature = "legacy-tests")]
#[path = "../test_regulatory_gate.rs"]
mod test_regulatory_gate;

#[cfg(feature = "legacy-tests")]
#[path = "../test_repayment_events_audit_parity.rs"]
mod test_repayment_events_audit_parity;

#[cfg(feature = "legacy-tests")]
#[path = "../test_replay_guard.rs"]
mod test_replay_guard;

#[cfg(feature = "legacy-tests")]
#[path = "../test_require_business_active.rs"]
mod test_require_business_active;

#[cfg(feature = "legacy-tests")]
#[path = "../test_require_valid_business_kyc_tier.rs"]
mod test_require_valid_business_kyc_tier;

#[cfg(feature = "legacy-tests")]
#[path = "../test_require_valid_invoice_category.rs"]
mod test_require_valid_invoice_category;

#[path = "../test_resolution_policy_enforcement.rs"]
mod test_resolution_policy_enforcement;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_resource_rate_limits.rs"]
mod test_resource_rate_limits;

#[cfg(feature = "legacy-tests")]
#[path = "../test_revenue_split.rs"]
mod test_revenue_split;

#[cfg(feature = "legacy-tests")]
#[path = "../test_risk_tier.rs"]
mod test_risk_tier;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_seed.rs"]
mod test_seed;

#[cfg(feature = "legacy-tests")]
#[path = "../test_self_call_rejection.rs"]
mod test_self_call_rejection;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settle_during_dispute.rs"]
mod test_settle_during_dispute;

#[path = "../test_settlement.rs"]
mod test_settlement;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settlement_accounting_identity.rs"]
mod test_settlement_accounting_identity;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settlement_atomic_rollback.rs"]
mod test_settlement_atomic_rollback;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settlement_auto_release.rs"]
mod test_settlement_auto_release;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settlement_capacity_stress.rs"]
mod test_settlement_capacity_stress;

#[cfg(all(test, feature = "fuzz-tests"))]
#[path = "../test_settlement_conservation_props.rs"]
mod test_settlement_conservation_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settlement_currency_whitelist.rs"]
mod test_settlement_currency_whitelist;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settlement_dispute_interaction.rs"]
mod test_settlement_dispute_interaction;

#[cfg(feature = "legacy-tests")]
#[path = "../test_settlement_history_reconstruction.rs"]
mod test_settlement_history_reconstruction;

#[cfg(feature = "legacy-tests")]
#[path = "../test_snapshot.rs"]
mod test_snapshot;

#[cfg(feature = "legacy-tests")]
#[allow(
    clippy::disallowed_methods,
    reason = "Test assertions report unexpected Result errors; production linting remains enabled."
)]
#[path = "../test_storage.rs"]
mod test_storage;

#[cfg(feature = "legacy-tests")]
#[path = "../test_storage_key_layout.rs"]
mod test_storage_key_layout;

#[cfg(feature = "legacy-tests")]
#[path = "../test_storage_migration_compat.rs"]
mod test_storage_migration_compat;

#[allow(clippy::disallowed_macros)]
#[path = "../test_storage_tier_migration.rs"]
mod test_storage_tier_migration;

#[path = "../test_storage_ttl.rs"]
mod test_storage_ttl;

#[cfg(feature = "legacy-tests")]
#[path = "../test_store_invoice_auth.rs"]
mod test_store_invoice_auth;

#[cfg(feature = "legacy-tests")]
#[path = "../test_store_invoices_batch.rs"]
mod test_store_invoices_batch;

#[cfg(feature = "legacy-tests")]
#[path = "../test_string_limits.rs"]
mod test_string_limits;

#[cfg(feature = "legacy-tests")]
#[path = "../test_tier_boundary.rs"]
mod test_tier_boundary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_treasury_rotation.rs"]
mod test_treasury_rotation;

#[cfg(feature = "legacy-tests")]
#[path = "../test_treasury_rotation_deadline.rs"]
mod test_treasury_rotation_deadline;

#[cfg(feature = "legacy-tests")]
#[path = "../test_treasury_split_overflow_props.rs"]
mod test_treasury_split_overflow_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_twa_props.rs"]
mod test_twa_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_types.rs"]
mod test_types;

#[cfg(feature = "legacy-tests")]
#[path = "../test_upgrade_guard.rs"]
mod test_upgrade_guard;

#[cfg(feature = "legacy-tests")]
#[path = "../test_verification_matrix.rs"]
mod test_verification_matrix;

#[cfg(feature = "legacy-tests")]
#[path = "../test_verify_bid_match.rs"]
mod test_verify_bid_match;

#[cfg(feature = "legacy-tests")]
#[path = "../test_vesting.rs"]
mod test_vesting;

#[cfg(feature = "legacy-tests")]
#[path = "../test_vesting_cliff_final.rs"]
mod test_vesting_cliff_final;

#[cfg(feature = "legacy-tests")]
#[path = "../test_vesting_summary.rs"]
mod test_vesting_summary;

#[cfg(feature = "legacy-tests")]
#[path = "../test_view_only.rs"]
mod test_view_only;

#[cfg(feature = "legacy-tests")]
#[allow(clippy::disallowed_macros)]
#[path = "../test_volume_tier_props.rs"]
mod test_volume_tier_props;

#[cfg(feature = "legacy-tests")]
#[path = "../test_withdraw_bid_matrix.rs"]
mod test_withdraw_bid_matrix;
