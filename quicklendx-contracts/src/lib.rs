#![no_std]

extern crate alloc; // Add this for Vec support in no_std

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

// Export essential modules for basic functionality
pub mod errors;
pub mod types;
pub mod invoice_amount;

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

// Constants that are used across modules
pub const MAX_QUERY_LIMIT: u32 = 100;

#[cfg(test)]
mod test_invoice_amount_precision;
#[cfg(test)]
mod test_platform_fee_ownership;

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
