//! Investment storage operations and index management.

use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, Vec, String};

use crate::types::{Investment, InvestmentStatus};
use super::{Indexes, StorageKeys, StorageIntegrityAudit};

pub use crate::investment::InvestmentStorage;

impl StorageKeys {
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"inv_cnt"` resets the investment counter on all deployed contracts.
    pub fn investment_count() -> Symbol {
        symbol_short!("inv_cnt")
    }
}

impl Indexes {
    /// Returns the persistent storage key for the investment list attached to an invoice.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"invst_inv"` orphans all per-invoice investment indexes.
    pub fn investments_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("invst_inv"), invoice_id.clone())
    }

    /// Returns the persistent storage key for the investment list owned by an investor.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"inv_invst"` orphans all per-investor investment indexes.
    pub fn investments_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("inv_invst"), investor.clone())
    }

    /// Returns the persistent storage key for the investment list in a given status bucket.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming any status symbol or `"inv_st"` here orphans that index.
    /// Note: this shares the `"inv_st"` prefix with `invoices_by_status`; the second
    /// tuple element (the status symbol) acts as the discriminator.
    pub fn investments_by_status(status: InvestmentStatus) -> (Symbol, Symbol) {
        let status_symbol = match status {
            InvestmentStatus::Active => symbol_short!("active"),
            InvestmentStatus::Withdrawn => symbol_short!("withdrawn"),
            InvestmentStatus::Completed => symbol_short!("completed"),
            InvestmentStatus::Defaulted => symbol_short!("defaulted"),
            InvestmentStatus::Refunded => symbol_short!("refunded"),
        };
        (symbol_short!("inv_st"), status_symbol)
    }

}

impl StorageIntegrityAudit {
    pub fn audit_investment_integrity(env: &Env) -> Result<(), Vec<String>> {
        let mut errors = Vec::new(env);
        let mut all_discovered = Vec::new(env);

        // 1. Check active index (should only contain Active investments)
        let active_ids = InvestmentStorage::get_active_investment_ids(env);
        for id in active_ids.iter() {
            if !all_discovered.contains(&id) {
                all_discovered.push_back(id.clone());
            }
            match InvestmentStorage::get(env, &id) {
                None => {
                    errors.push_back(String::from_str(
                        env,
                        "Orphan ID in active investment index",
                    ));
                }
                Some(inv) => {
                    if inv.status != InvestmentStatus::Active {
                        errors.push_back(String::from_str(
                            env,
                            "Terminal investment found in active index",
                        ));
                    }
                }
            }
        }

        // 2. Cross-check consistency for discovered investments
        for id in all_discovered.iter() {
            if let Some(inv) = InvestmentStorage::get(env, &id) {
                // Check investor index
                let investor_ids = InvestmentStorage::get_by_investor(env, &inv.investor);
                if !investor_ids.contains(&id) {
                    errors.push_back(String::from_str(
                        env,
                        "Investment missing from investor index",
                    ));
                }

                // Check invoice mapping
                if let Some(mapped_id) =
                    InvestmentStorage::get_investment_by_invoice(env, &inv.invoice_id)
                {
                    if mapped_id.investment_id != id.clone() {
                        errors.push_back(String::from_str(
                            env,
                            "Invoice to investment mapping mismatch",
                        ));
                    }
                } else {
                    errors.push_back(String::from_str(
                        env,
                        "Investment missing from invoice mapping",
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

}
