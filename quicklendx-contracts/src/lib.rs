#![no_std]

extern crate alloc; // Add this for Vec support in no_std

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

pub mod address_summary;
pub mod admin;
pub mod analytics;
pub mod arbiter;
pub mod audit;
pub mod backup;
pub mod bid;
pub mod contract;
pub mod currency;
pub mod defaults;
pub mod diagnostics;
pub mod dispute;
pub mod dispute_timeline;
pub mod emergency;
pub mod errors;
pub mod events;
pub mod escrow;
pub mod fees;
pub mod freshness;
pub mod governance;
pub mod health;
pub mod idempotency;
pub mod incident;
pub mod init;
pub mod investment;
pub mod investment_queries;
pub mod invoice;
pub mod types;
pub mod invoice_amount;
pub mod invoice_search;
pub mod kyc_policy;
pub mod maintenance;
pub mod monitor;
pub mod multisig;
pub mod notifications;
pub mod observability;
pub mod operational_limits;
pub mod pagination;
pub mod pause;
pub mod payment_token_policy;
pub mod payments;
pub mod profits;
pub mod protocol_limits;
pub mod regulatory;
pub mod reentrancy;
pub mod resolution_policy;
pub mod settlement;
pub mod storage;
pub mod upgrade;
pub mod verification;
pub mod vesting;

pub use contract::{QuickLendXContract, QuickLendXContractClient};
// Constants that are used across modules
pub const MAX_QUERY_LIMIT: u32 = 100;

#[cfg(test)]
mod test_invoice_amount_precision;
#[cfg(test)]
mod test_escrow;
#[cfg(test)]
mod test_accept_bid_race;
#[cfg(test)]
mod test_bid_acceptance_replay;

/// Shared test utilities for contract regression coverage (issue #2711 CI
/// fix). Public (not `#[cfg(test)]`) so integration tests under `tests/`
/// can reach it via `quicklendx_contracts::test_utils`.
pub mod test_utils;

