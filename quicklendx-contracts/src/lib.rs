#![no_std]

extern crate alloc; // Add this for Vec support in no_std

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

// Export essential modules for basic functionality
pub mod errors;
pub mod escrow;
pub mod events;
pub mod fees;
pub mod freshness;
pub mod governance;
pub mod health;
pub mod incident;
pub mod init;
pub mod invariants;
pub mod idempotency;
pub mod investment;
pub mod investment_queries;
pub mod invoice;
pub mod invoice_search;
pub mod maintenance;
pub mod monitor;
pub mod notifications;
pub mod operational_limits;
pub mod pagination;
pub mod panic_handler;
pub mod pause;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod reentrancy;
pub mod settlement;
pub mod storage;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_accept_bid_instruction_budget;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_accept_bid_race;
#[cfg(test)]
mod test_panic_handler;
#[cfg(test)]
mod test_due_date_guard;
#[cfg(test)]
mod test_cancel_invoice_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_simple;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_standalone;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_admin_two_step;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_audit;
#[cfg(test)]
mod test_audit_config;
#[cfg(test)]
mod test_rating_override;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup_restore_reindex;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup_retention_enforcement;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backup_safety;
#[cfg(test)]
mod test_bid_cancel_accept_race;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_expiry_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_ttl;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_cleanup_pagination;
#[cfg(test)]
mod test_config_bounds_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_currency;
#[cfg(test)]
mod test_currency_batch;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_currency_match_funding;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute;
#[cfg(test)]
mod test_dispute_refund_flow;
#[cfg(test)]
mod test_evidence_size_cap;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_dispute_timeline_props;
#[cfg(test)]
// mod test_dispute_event_invariant;
#[cfg(test)]
mod test_dust_transfer;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_event_completeness;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_invariant_model;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_escrow_refund_after_expiry;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_expired_bids_cleanup;
#[cfg(test)]
// mod test_freshness;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freshness_bounds;
#[cfg(test)]
mod test_payments;
#[cfg(test)]
mod test_queries;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_self_call_rejection;
// Issue #1541 — lag at zero, lag at positive, lag during pause.
#[cfg(all(test, feature = "legacy-tests"))]
mod test_freshness_lag;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_health_status;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_init;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invariant_self_check;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_consistency;
#[cfg(test)]
mod test_operational_limits;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_withdraw_bid_matrix;
// #[cfg(test)]
#[cfg(test)]
#[path = "test/test_investment_queries.rs"]
mod test_investment_queries;
// #[cfg(all(test, feature = "legacy-tests"))]
// mod test_overflow;
// #[cfg(all(test, feature = "legacy-tests"))]
// mod test_pause;
// #[cfg(all(test, feature = "legacy-tests"))]
// mod test_profit_fee;
// #[cfg(all(test, feature = "legacy-tests"))]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_backpressure_shedding;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_profit_fee;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_protocol_health;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_protocol_limits_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
// mod test_refund;
// #[cfg(all(test, feature = "legacy-tests"))]
// mod test_storage;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_reentrancy;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_reentrancy_fault_injection;
#[cfg(test)]
mod test_settlement_accounting_identity;
#[cfg(test)]
mod test_storage_key_layout;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_string_limits;
// #[cfg(all(test, feature = "legacy-tests"))]
// mod test_types;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_analytics_consistency;
#[cfg(test)]
mod test_bid_capacity_stress;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_bid_compare_order_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_bid_ranking;
#[cfg(test)]
mod test_vesting;
mod test_vesting_summary;
// Issue #1551 — determinism tests for bid_ranking; no feature gate, runs on
// every CI matrix entry.
#[cfg(test)]
mod test_bid_ranking_determinism;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_business_invoices_paged_ordering;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_category_breakdown;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_clock_rollover;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_compute_yield_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_default_grace_boundary;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_overdue_scan_resilience;
#[cfg(test)]
mod test_diagnostics;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_events;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_fuzz_cancelled_noop;
#[cfg(all(test, feature = "legacy-tests", feature = "fuzz-tests"))]
mod test_fuzz_distribute_revenue;
#[cfg(all(test, feature = "legacy-tests", feature = "fuzz-tests"))]
mod test_fuzz_invoice_metadata;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_fuzz_partial_payment;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_incident;
#[cfg(test)]
#[cfg(all(test, feature = "legacy-tests"))]
mod test_init_invariants;
#[cfg(test)]
mod test_input_matrix;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_withdrawal;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_investment_transitions;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_metadata;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_line_item_consistency;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_invoice_search_ranking;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_rebuild_indexes;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_max_invoices_per_business;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_insurance_claim_payout;
#[cfg(test)]
mod test_insurance_optin_lifecycle;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_insurance_premium_props;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_twa_props;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_notifications;
#[cfg(all(test, feature = "legacy-tests"))]
mod test_pause_reads_available;
mod test_platform_metrics_reconciliation;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_seed;
#[cfg(all(test, feature = "legacy-tests", feature = "fuzz-tests"))]
mod test_treasury_split_overflow_props;
#[cfg(all(test, feature = "fuzz-tests"))]
mod test_volume_tier_props;
// Issue #1482 — "cannot withdraw more than deposited" invariant: hard-coded sad
// path (always runs) + proptest property (requires fuzz-tests feature).
#[cfg(test)]
mod test_cannot_withdraw_more_than_deposited;
#[cfg(test)]
mod test_tier_boundary;
pub mod types;
pub mod invoice_amount;

// Constants that are used across modules
pub const MAX_QUERY_LIMIT: u32 = 100;

#[cfg(test)]
mod test_invoice_amount_precision;

/// Shared test utilities for contract regression coverage (issue #2711 CI
/// fix). Public (not `#[cfg(test)]`) so integration tests under `tests/`
/// can reach it via `quicklendx_contracts::test_utils`.
pub mod test_utils;

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn hello(_env: Env) -> Symbol {
        symbol_short!("HELLO")
    }
}