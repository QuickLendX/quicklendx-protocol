#![no_std]
use soroban_sdk::{contract, contractimpl, Env};
use crate::errors::QuickLendXError; // Fixes the import error

pub mod admin;
pub mod errors;
pub mod events;
pub mod fees;
pub mod init;
pub mod pause;
pub mod profits;
pub mod settlement;
pub mod storage_types;
pub mod verification;
pub mod payments;
pub mod invariants;
pub mod types;

// Hardcoded constant to break the circular dependency
pub(crate) const MAX_QUERY_LIMIT: u32 = 100; 

#[contract]
pub struct QuickLendX;

#[contractimpl]
impl QuickLendX {
    // This is the structure your project expects
    // Add your existing functions here or ensure they match this structure
}