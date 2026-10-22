#![no_std]

extern crate alloc; // Add this for Vec support in no_std

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
pub mod payments;

// The legacy contract modules are not wired into the deployment surface yet,
// but platform-fee ownership tests exercise them directly. Compile only the
// modules needed for that regression suite in test builds so the production
// WASM stays slim.
#[cfg(test)]
pub mod admin;
#[cfg(test)]
pub mod audit;
#[cfg(test)]
pub mod bid;
#[cfg(test)]
pub mod diagnostics;
#[cfg(test)]
pub mod events;
#[cfg(test)]
pub mod fees;
#[cfg(test)]
pub mod governance;
#[cfg(test)]
pub mod investment;
#[cfg(test)]
pub mod observability;
#[cfg(test)]
pub mod pagination;
#[cfg(test)]
pub mod payments;
#[cfg(test)]
pub mod profits;
#[cfg(test)]
pub mod protocol_limits;
#[cfg(test)]
pub mod storage;
#[cfg(test)]
pub mod verification;

#[cfg(test)]
pub use admin::AdminStorage;

// Protocol initialization and admin management (issue #2925) and the modules
// they depend on. Other modules are declared as their entrypoints are wired.
pub mod admin;
pub mod audit;
pub mod bid;
pub mod diagnostics;
pub mod events;
pub mod fees;
pub mod governance;
pub mod init;
pub mod investment;
pub mod observability;
pub mod pagination;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod storage;
pub mod verification;

mod contract;

pub use admin::AdminStorage;
pub use contract::{QuickLendXContract, QuickLendXContractClient};

// Modules behind the wired entrypoints (protocol initialization and admin,
// issue #2925; invoice creation and lifecycle, issue #2926) and the modules
// they depend on. Other modules are declared as their entrypoints are wired.
pub mod admin;
pub mod audit;
pub mod bid;
pub mod currency;
pub mod defaults;
pub mod diagnostics;
pub mod events;
pub mod fees;
pub mod governance;
pub mod idempotency;
pub mod init;
pub mod invoice;
pub mod investment;
pub mod maintenance;
pub mod notifications;
pub mod observability;
pub mod pagination;
pub mod pause;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod regulatory;
pub mod settlement;
pub mod storage;
pub mod upgrade;
pub mod backup;
pub mod backup_v1;
pub mod verification;

mod contract;

pub use admin::AdminStorage;
pub use contract::{QuickLendXContract, QuickLendXContractClient};

// Constants that are used across modules
pub const MAX_QUERY_LIMIT: u32 = 100;

#[cfg(test)]
mod test_invoice_amount_precision;
#[cfg(test)]
mod test_init;
#[cfg(test)]
mod test_admin_two_step;
#[cfg(test)]
mod test_contract_init_admin;
#[cfg(test)]
mod test_invoice;
#[cfg(test)]
mod test_invoice_metadata;

/// Shared test utilities for contract regression coverage (issue #2711 CI
/// fix). Public (not `#[cfg(test)]`) so integration tests under `tests/`
/// can reach it via `quicklendx_contracts::test_utils`.
pub mod test_utils;
