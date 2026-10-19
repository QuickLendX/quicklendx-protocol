#![no_std]

extern crate alloc; // Add this for Vec support in no_std

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

// Export essential modules for basic functionality
pub mod errors;
/// Invoice amount precision and overflow validation (Issue #2432).
///
/// See the module docs for the exact integer rules, invariants, compatibility
/// impact, and security assumptions. The invoice lifecycle entrypoints
/// (`contract.rs::store_invoice`, `invoice.rs::Invoice::new`) route their
/// amount checks through this module.
pub mod invoice_amount;
pub mod types;

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