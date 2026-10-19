#![no_std]

extern crate alloc; // Add this for Vec support in no_std

// Export essential modules for basic functionality
pub mod errors;
pub mod types;
pub mod invoice_amount;

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
