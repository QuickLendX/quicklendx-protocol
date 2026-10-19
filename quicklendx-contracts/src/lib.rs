#![no_std]
#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_comparisons,
    deprecated,
    clippy::too_many_arguments,
    clippy::doc_overindented_list_items,
    clippy::absurd_extreme_comparisons,
    clippy::needless_range_loop,
    clippy::manual_checked_ops,
    clippy::collapsible_match,
    clippy::let_unit_value,
    clippy::needless_borrow,
    clippy::match_like_matches_macro,
    clippy::needless_return,
    clippy::disallowed_methods
)]

extern crate alloc; // Add this for Vec support in no_std

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

pub use crate::admin::AdminStorage;
pub use crate::errors::QuickLendXError;

// Export essential modules for basic functionality
pub mod errors;
pub mod types;
pub mod invoice_amount;

// Constants that are used across modules
pub const MAX_QUERY_LIMIT: u32 = 100;

#[cfg(test)]
mod test_invoice_amount_precision;

/// Resolution-policy enforcement regression tests (issue #2963).
#[cfg(test)]
mod test_resolution_policy_enforcement;

/// Shared test utilities for contract regression coverage (issue #2711 CI
/// fix). Public (not `#[cfg(test)]`) so integration tests under `tests/`
/// can reach it via `quicklendx_contracts::test_utils`.
pub mod test_utils;

pub mod address_summary;
pub mod admin;
pub mod analytics;
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
pub mod diagnostics;
pub mod dispute;
pub mod dispute_timeline;
pub mod emergency;
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
pub mod invoice_search;
pub mod maintenance;
pub mod monitor;
pub mod multisig;
pub mod notifications;
pub mod observability;
pub mod operational_limits;
pub mod pagination;
pub mod panic_handler;
pub mod pause;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod reentrancy;
pub mod regulatory;
pub mod resolution_policy;
pub mod settlement;
pub mod storage;
pub mod upgrade;
pub mod verification;
pub mod vesting;

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn hello(_env: Env) -> Symbol {
        symbol_short!("HELLO")
    }
}
