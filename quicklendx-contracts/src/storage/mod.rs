//! Storage management for the QuickLendX invoice factoring protocol.
//!
//! This module defines storage keys, indexing strategies, and storage operations
//! for efficient data retrieval and management across submodules.

pub mod bid;
pub mod config;
pub mod investment;
pub mod invoice;
pub mod migration;

pub use bid::*;
pub use config::*;
pub use investment::*;
pub use invoice::*;
pub use migration::*;

use soroban_sdk::{contracttype, Address, BytesN, Env, String, Vec};

/// Counter and configuration keys for the contract.
///
/// # BREAKING: Rename Requires Migration
///
/// Each method here produces a storage key persisted on-chain.  Renaming the
/// inner `symbol_short!` string is a **breaking change**: the value stored under
/// the old key becomes permanently unreadable.  See `docs/storage-key-stability.md`
/// for the migration checklist.
pub struct StorageKeys;

/// Secondary indexes for efficient querying.
///
/// # BREAKING: Rename Requires Migration
///
/// Every method in this struct produces a storage key that is persisted on-chain.
/// Renaming the inner `symbol_short!` string in **any** method is a **breaking change**:
/// existing contract data stored under the old key becomes permanently unreachable
/// (orphaned) unless a migration function explicitly copies it to the new key.
pub struct Indexes;

/// Primary storage key namespace for core entities.
///
/// # BREAKING: Rename Requires Migration
///
/// Renaming a variant discriminant (e.g., `Invoice` -> `Inv`) changes the
/// XDR encoding of every key in that variant, orphaning all existing records.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Invoice(BytesN<32>),
    Bid(BytesN<32>),
    Investment(BytesN<32>),
    FrozenInvoice(BytesN<32>),
    FreezeInfo(BytesN<32>),
    EscrowExtension(BytesN<32>),
    InvestorFreezeInfo(Address),
    PerInvestorPositionCap(BytesN<32>),
    /// Content-addressed dispute evidence bound to its owning invoice.
    DisputeEvidence(BytesN<32>),
}

/// Bounded integrity auditing for storage indexes.
pub struct StorageIntegrityAudit;

impl StorageIntegrityAudit {
    /// Performs a full protocol-wide integrity audit across invoices, bids, and investments.
    pub fn audit_all(env: &Env) -> Result<(), Vec<String>> {
        let mut all_errors = Vec::new(env);

        if let Err(e) = Self::audit_invoice_integrity(env) {
            for err in e.iter() {
                all_errors.push_back(err.clone());
            }
        }
        if let Err(e) = Self::audit_bid_integrity(env) {
            for err in e.iter() {
                all_errors.push_back(err.clone());
            }
        }
        if let Err(e) = Self::audit_investment_integrity(env) {
            for err in e.iter() {
                all_errors.push_back(err.clone());
            }
        }

        if all_errors.is_empty() {
            Ok(())
        } else {
            Err(all_errors)
        }
    }
}
