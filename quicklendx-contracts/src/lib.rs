#![no_std]

extern crate alloc; // Add this for Vec support in no_std

// Export essential modules for basic functionality
pub mod errors;
pub mod types;
pub mod invoice_amount;

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

/// Shared test utilities for contract regression coverage (issue #2711 CI
/// fix). Public (not `#[cfg(test)]`) so integration tests under `tests/`
/// can reach it via `quicklendx_contracts::test_utils`.
pub mod test_utils;
