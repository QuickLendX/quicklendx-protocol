#![no_std]

use soroban_sdk::{contract, contractimpl, Env, Symbol, symbol_short};

pub mod errors;
/// Invoice amount precision and overflow validation (Issue #2432).
///
/// See the module docs for the exact integer rules, invariants, compatibility
/// impact, and security assumptions. The invoice lifecycle entrypoints
/// (`contract.rs::store_invoice`, `invoice.rs::Invoice::new`) route their
/// amount checks through this module.
pub mod invoice_amount;

#[cfg(test)]
mod test_invoice_amount_precision;

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    pub fn hello(env: Env) -> Symbol {
        symbol_short!("A1")
    }
}
