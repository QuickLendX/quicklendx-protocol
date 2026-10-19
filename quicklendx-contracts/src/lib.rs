#`![no_std]

extern crate alloc; // Add this for Vec support in no_std

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

// Export essential modules for basic functionality
pub mod errors;
pub mod types;
pub mod invoice_amount;

// Production modules (every non-test module in src/ except contract.rs)
pub mod admin;
pub mod address_summary;
pub mod audit;
pub mod bid;
pub mod dispute;
pub mod escrow;
pub mod fees;
pub mod idempotency;
pub mod init;
pub mod invoice;
pub mod kyc_policy;
pub mod payment_token_policy;
pub mod payments;
pub mod settlement;
pub mod verification;

// Benchmark code only compiles for tests or the testutils feature.
#[cfg(any(test, feature = "testutils"))]
pub mod bench;

// Constants that are used across modules
pub const MAX_QUERY_LIMIT: u32 = 100;

#[cfg(test)]
mod test_invoice_amount_precision;

/// Shared test utilities for contract regression coverage (issue #2711 CI 
/// fix). Public (not `#[cfg(test)]') so integration tests under `tests/`
pub mod test_utils;

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn hello(_env: Env) -> Symbol {
        symbol_short!("HELLO")
    }
}
