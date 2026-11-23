#![no_std]
use crate::errors::QuickLendXError;
use soroban_sdk::{contract, contractimpl, Env}; // Fixes the import error

pub mod admin;
pub mod errors;
pub mod events;
pub mod fees;
pub mod init;
pub mod invariants;
pub mod kyc_nonces;
pub mod kyc_policy;
pub mod pause;
pub mod payment_token_policy;
pub mod payments;
pub mod profits;
pub mod settlement;
pub mod storage_types;
#[cfg(test)]
mod test_kyc_policy_entrypoints;
#[cfg(test)]
mod test_kyc_policy_extended;
#[cfg(test)]
mod test_kyc_policy_matrix;
#[cfg(test)]
mod test_payment_token_policy;
#[cfg(test)]
mod test_payment_token_policy_batch;
#[cfg(test)]
mod test_payment_token_policy_matrix;
#[cfg(test)]
mod test_payment_token_policy_regression;
pub mod types;
pub mod verification;

// Hardcoded constant to break the circular dependency
pub(crate) const MAX_QUERY_LIMIT: u32 = 100;

#[contract]
pub struct QuickLendX;

#[contractimpl]
impl QuickLendX {
    // This is the structure your project expects
    // Add your existing functions here or ensure they match this structure
}
