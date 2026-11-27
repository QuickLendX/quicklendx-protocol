//! Bid storage operations and index management.

use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, Vec, String};

use crate::types::{Bid, BidStatus};
use super::{Indexes, StorageKeys, StorageIntegrityAudit};

pub use crate::bid::BidStorage;

impl StorageKeys {
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"bid_count"` resets the bid counter on all deployed contracts.
    pub fn bid_count() -> Symbol {
        symbol_short!("bid_count")
    }
}

impl Indexes {
    /// Returns the persistent storage key for the bid list attached to an invoice.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"bids_inv"` orphans all per-invoice bid indexes.
    pub fn bids_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("bids_inv"), invoice_id.clone())
    }

    /// Returns the persistent storage key for the bid list owned by an investor.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"bids_invr"` orphans all per-investor bid indexes.
    pub fn bids_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("bids_invr"), investor.clone())
    }

    /// Returns the persistent storage key for the bid list in a given status bucket.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming any status symbol or `"bids_stat"` orphans that status index.
    pub fn bids_by_status(status: BidStatus) -> (Symbol, Symbol) {
        let status_symbol = match status {
            BidStatus::Placed => symbol_short!("placed"),
            BidStatus::Withdrawn => symbol_short!("withdrawn"),
            BidStatus::Accepted => symbol_short!("accepted"),
            BidStatus::Expired => symbol_short!("expired"),
            BidStatus::Cancelled => symbol_short!("cancelled"),
        };
        (symbol_short!("bids_stat"), status_symbol)
    }

}

impl StorageIntegrityAudit {
    pub fn audit_bid_integrity(env: &Env) -> Result<(), Vec<String>> {
        let mut errors = Vec::new(env);
        let all_bid_ids = BidStorage::get_all_bids(env);

        for bid_id in all_bid_ids.iter() {
            match BidStorage::get_bid(env, &bid_id) {
                None => {
                    errors.push_back(String::from_str(env, "Orphan ID in global bid list"));
                }
                Some(bid) => {
                    // Check investor index
                    let investor_ids = BidStorage::get_bids_by_investor_all(env, &bid.investor);
                    if !investor_ids.contains(&bid_id) {
                        errors.push_back(String::from_str(env, "Bid missing from investor index"));
                    }

                    // Check invoice index
                    let invoice_ids = BidStorage::get_bids_for_invoice(env, &bid.invoice_id);
                    if !invoice_ids.contains(&bid_id) {
                        errors.push_back(String::from_str(env, "Bid missing from invoice index"));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Audits all investment-related indexes.
}
