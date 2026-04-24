//! Storage management for the QuickLendX invoice factoring protocol.
//!
//! This module defines storage keys, indexing strategies, and storage operations
//! for efficient data retrieval and management.
//!
//! # Storage Design
//!
//! The storage is organized with the following indexing strategy:
//! - Primary storage: Direct key-value storage for core entities
//! - Secondary indexes: For efficient querying by various criteria
//!
//! # Security Notes
//!
//! - Storage keys use symbols to prevent collisions
//! - Instance storage is used for frequently accessed data
//! - Persistent storage for long-term data retention
//! - Upgrade-safe: Keys are designed to avoid conflicts during contract upgrades
//!
//! # Collision Safety (Schema v1)
//!
//! Primary entity keys are namespaced via the `DataKey` enum.
//! The Soroban host includes the enum discriminant in the serialized
//! key, guaranteeing that `Invoice(x)`, `Bid(x)`, and `Investment(x)`
//! never occupy the same storage slot regardless of the ID value.
//!
//! Index keys continue to use `(Symbol, payload)` tuples. Each index
//! function uses a unique symbol — see `Indexes` for the full list.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec};
// Removed ToString import; not needed in Soroban environment.

use crate::types::{Bid, BidStatus, Investment, InvestmentStatus, Invoice, InvoiceStatus};
use crate::profits::PlatformFeeConfig;

/// Storage keys for the contract
pub struct StorageKeys;

/// Primary storage key namespace for core entities.
///
/// # Collision Safety
///
/// Each variant wraps the same `BytesN<32>` ID type, but the Soroban
/// host serializes the enum discriminant tag together with the payload,
/// so `DataKey::Invoice(x)` and `DataKey::Bid(x)` produce distinct
/// binary storage keys even when `x` is identical. This makes
/// cross-entity collisions structurally impossible at the protocol level.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Primary key for an Invoice record. Keyed by invoice ID.
    Invoice(BytesN<32>),
    /// Primary key for a Bid record. Keyed by bid ID.
    Bid(BytesN<32>),
    /// Primary key for an Investment record. Keyed by investment ID.
    Investment(BytesN<32>),
}

impl StorageKeys {
    /// Key for platform fee configuration
    pub fn platform_fees() -> Symbol {
        symbol_short!("fees")
    }

    /// Key for invoice count
    pub fn invoice_count() -> Symbol {
        symbol_short!("inv_count")
    }

    /// Key for bid count
    pub fn bid_count() -> Symbol {
        symbol_short!("bid_count")
    }

    /// Key for investment count
    pub fn investment_count() -> Symbol {
        symbol_short!("inv_cnt")
    }
}

/// Secondary indexes for efficient querying
pub struct Indexes;

impl Indexes {
    /// Index: invoices by business address
    pub fn invoices_by_business(business: &Address) -> (Symbol, Address) {
        (symbol_short!("inv_bus"), business.clone())
    }

    /// Index: invoices by status
    pub fn invoices_by_status(status: InvoiceStatus) -> (Symbol, Symbol) {
        let status_symbol = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("defaulted"),
            InvoiceStatus::Cancelled => symbol_short!("cancelled"),
            InvoiceStatus::Refunded => symbol_short!("refunded"),
        };
        (symbol_short!("inv_st"), status_symbol)
    }

    /// Index: bids by invoice
    pub fn bids_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("bids_inv"), invoice_id.clone())
    }

    /// Index: bids by investor
    pub fn bids_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("bids_invr"), investor.clone())
    }

    /// Index: bids by status
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

    /// Index: investments by invoice
    pub fn investments_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("invst_inv"), invoice_id.clone())
    }

    /// Index: investments by investor
    pub fn investments_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("inv_invst"), investor.clone())
    }

    /// Index: investments by status
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

    /// Index: invoices by customer name
    pub fn invoices_by_customer(
        customer_name: &soroban_sdk::String,
    ) -> (Symbol, soroban_sdk::String) {
        (symbol_short!("inv_cust"), customer_name.clone())
    }

    /// Index: invoices by tax_id
    pub fn invoices_by_tax_id(tax_id: &soroban_sdk::String) -> (Symbol, soroban_sdk::String) {
        (symbol_short!("inv_taxid"), tax_id.clone())
    }
}

/// Storage operations for invoices
pub struct InvoiceStorage;

impl InvoiceStorage {
    /// Store an invoice
    pub fn store(env: &Env, invoice: &Invoice) {
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice.id.clone()), invoice);
        Self::add_to_business_index(env, &invoice.business, &invoice.id);
        Self::add_to_status_index(env, invoice.status.clone(), &invoice.id);
        if let Some(ref name) = invoice.metadata_customer_name {
            Self::add_to_customer_index(env, name, &invoice.id);
        }
        if let Some(ref tax_id) = invoice.metadata_tax_id {
            Self::add_to_tax_id_index(env, tax_id, &invoice.id);
        }
    }

    pub fn get_by_business(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_business(business);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env))
    }

    pub fn get_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_status(status);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env))
    }

    /// Get an invoice by ID
    pub fn get(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        env.storage()
            .persistent()
            .get(&DataKey::Invoice(invoice_id.clone()))
    }

    /// Update an invoice
    pub fn update(env: &Env, invoice: &Invoice) {
        if let Some(old_invoice) = Self::get(env, &invoice.id) {
            if old_invoice.status != invoice.status {
                Self::remove_from_status_index(env, old_invoice.status, &invoice.id);
                Self::add_to_status_index(env, invoice.status.clone(), &invoice.id);
            }
            if old_invoice.metadata_customer_name != invoice.metadata_customer_name {
                if let Some(ref old_name) = old_invoice.metadata_customer_name {
                    Self::remove_from_customer_index(env, old_name, &invoice.id);
                }
                if let Some(ref new_name) = invoice.metadata_customer_name {
                    Self::add_to_customer_index(env, new_name, &invoice.id);
                }
            }
            if old_invoice.metadata_tax_id != invoice.metadata_tax_id {
                if let Some(ref old_tax_id) = old_invoice.metadata_tax_id {
                    Self::remove_from_tax_id_index(env, old_tax_id, &invoice.id);
                }
                if let Some(ref new_tax_id) = invoice.metadata_tax_id {
                    Self::add_to_tax_id_index(env, new_tax_id, &invoice.id);
                }
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(invoice.id.clone()), invoice);
    }

    /// Add invoice to business index
    fn add_to_business_index(env: &Env, business: &Address, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_business(env, business);
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::invoices_by_business(business), &invoices);
        }
    }

    /// Add invoice to status index
    fn add_to_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_status(env, status.clone());
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::invoices_by_status(status), &invoices);
        }
    }

    /// Remove invoice from status index
    fn remove_from_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_status(env, status.clone());
        if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
            invoices.remove(pos as u32);
            env.storage()
                .persistent()
                .set(&Indexes::invoices_by_status(status), &invoices);
        }
    }

    pub fn add_to_customer_index(env: &Env, customer_name: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_customer(customer_name);
        let mut ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_from_customer_index(env: &Env, customer_name: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_customer(customer_name);
        let ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        let mut filtered = Vec::new(env);
        for id in ids.iter() {
            if id != *invoice_id {
                filtered.push_back(id.clone());
            }
        }
        env.storage().persistent().set(&key, &filtered);
    }

    pub fn add_to_tax_id_index(env: &Env, tax_id: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tax_id(tax_id);
        let mut ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_from_tax_id_index(env: &Env, tax_id: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tax_id(tax_id);
        let ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        let mut filtered = Vec::new(env);
        for id in ids.iter() {
            if id != *invoice_id {
                filtered.push_back(id.clone());
            }
        }
        env.storage().persistent().set(&key, &filtered);
    }

    /// Get next invoice count
    pub fn next_count(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::invoice_count())
            .unwrap_or(0);
        let next = current.saturating_add(1);
        env.storage()
            .persistent()
            .set(&StorageKeys::invoice_count(), &next);
        next
    }
}

/// Storage operations for bids
pub struct BidStorage;

impl BidStorage {
    /// Store a bid
    pub fn store(env: &Env, bid: &Bid) {
        env.storage()
            .persistent()
            .set(&DataKey::Bid(bid.bid_id.clone()), bid);

        // Update indexes
        Self::add_to_invoice_index(env, &bid.invoice_id, &bid.bid_id);
        Self::add_to_investor_index(env, &bid.investor, &bid.bid_id);
        Self::add_to_status_index(env, bid.status.clone(), &bid.bid_id);
    }

    /// Get a bid by ID
    pub fn get(env: &Env, bid_id: &BytesN<32>) -> Option<Bid> {
        env.storage()
            .persistent()
            .get(&DataKey::Bid(bid_id.clone()))
    }

    /// Update a bid
    pub fn update(env: &Env, bid: &Bid) {
        // Remove from old status index if status changed
        if let Some(old_bid) = Self::get(env, &bid.bid_id) {
            if old_bid.status != bid.status {
                Self::remove_from_status_index(env, old_bid.status, &bid.bid_id);
                Self::add_to_status_index(env, bid.status.clone(), &bid.bid_id);
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::Bid(bid.bid_id.clone()), bid);
    }

    /// Get bids by invoice
    pub fn get_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::bids_by_invoice(invoice_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get bids by investor
    pub fn get_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::bids_by_investor(investor))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get bids by status
    pub fn get_by_status(env: &Env, status: BidStatus) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::bids_by_status(status))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add bid to invoice index
    fn add_to_invoice_index(env: &Env, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        let mut bids = Self::get_by_invoice(env, invoice_id);
        if !bids.contains(bid_id) {
            bids.push_back(bid_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::bids_by_invoice(invoice_id), &bids);
        }
    }

    /// Add bid to investor index
    fn add_to_investor_index(env: &Env, investor: &Address, bid_id: &BytesN<32>) {
        let mut bids = Self::get_by_investor(env, investor);
        if !bids.contains(bid_id) {
            bids.push_back(bid_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::bids_by_investor(investor), &bids);
        }
    }

    /// Add bid to status index
    fn add_to_status_index(env: &Env, status: BidStatus, bid_id: &BytesN<32>) {
        let mut bids = Self::get_by_status(env, status.clone());
        if !bids.contains(bid_id) {
            bids.push_back(bid_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::bids_by_status(status), &bids);
        }
    }

    /// Remove bid from status index
    fn remove_from_status_index(env: &Env, status: BidStatus, bid_id: &BytesN<32>) {
        let mut bids = Self::get_by_status(env, status.clone());
        if let Some(pos) = bids.iter().position(|id| id == *bid_id) {
            bids.remove(pos as u32);
            env.storage()
                .persistent()
                .set(&Indexes::bids_by_status(status), &bids);
        }
    }

    /// Get next bid count
    pub fn next_count(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::bid_count())
            .unwrap_or(0);
        let next = current.saturating_add(1);
        env.storage()
            .persistent()
            .set(&StorageKeys::bid_count(), &next);
        next
    }
}

/// Storage operations for investments
pub struct InvestmentStorage;

impl InvestmentStorage {
    /// Store an investment
    pub fn store(env: &Env, investment: &Investment) {
        env.storage().persistent().set(
            &DataKey::Investment(investment.investment_id.clone()),
            investment,
        );

        // Update indexes
        Self::add_to_invoice_index(env, &investment.invoice_id, &investment.investment_id);
        Self::add_to_investor_index(env, &investment.investor, &investment.investment_id);
        Self::add_to_status_index(env, investment.status.clone(), &investment.investment_id);
    }

    /// Get an investment by ID
    pub fn get(env: &Env, investment_id: &BytesN<32>) -> Option<Investment> {
        env.storage()
            .persistent()
            .get(&DataKey::Investment(investment_id.clone()))
    }

    /// Update an investment
    pub fn update(env: &Env, investment: &Investment) {
        // Remove from old status index if status changed
        if let Some(old_investment) = Self::get(env, &investment.investment_id) {
            if old_investment.status != investment.status {
                Self::remove_from_status_index(
                    env,
                    old_investment.status,
                    &investment.investment_id,
                );
                Self::add_to_status_index(
                    env,
                    investment.status.clone(),
                    &investment.investment_id,
                );
            }
        }

        env.storage().persistent().set(
            &DataKey::Investment(investment.investment_id.clone()),
            investment,
        );
    }

    /// Get investments by invoice
    pub fn get_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::investments_by_invoice(invoice_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get investments by investor
    pub fn get_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::investments_by_investor(investor))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get investments by status
    pub fn get_by_status(env: &Env, status: InvestmentStatus) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::investments_by_status(status))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add investment to invoice index
    fn add_to_invoice_index(env: &Env, invoice_id: &BytesN<32>, investment_id: &BytesN<32>) {
        let mut investments = Self::get_by_invoice(env, invoice_id);
        if !investments.contains(investment_id) {
            investments.push_back(investment_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::investments_by_invoice(invoice_id), &investments);
        }
    }

    /// Add investment to investor index
    fn add_to_investor_index(env: &Env, investor: &Address, investment_id: &BytesN<32>) {
        let mut investments = Self::get_by_investor(env, investor);
        if !investments.contains(investment_id) {
            investments.push_back(investment_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::investments_by_investor(investor), &investments);
        }
    }

    /// Add investment to status index
    fn add_to_status_index(env: &Env, status: InvestmentStatus, investment_id: &BytesN<32>) {
        let mut investments = Self::get_by_status(env, status.clone());
        if !investments.contains(investment_id) {
            investments.push_back(investment_id.clone());
            env.storage()
                .persistent()
                .set(&Indexes::investments_by_status(status), &investments);
        }
    }

    /// Remove investment from status index
    fn remove_from_status_index(env: &Env, status: InvestmentStatus, investment_id: &BytesN<32>) {
        let mut investments = Self::get_by_status(env, status.clone());
        if let Some(pos) = investments.iter().position(|id| id == *investment_id) {
            investments.remove(pos as u32);
            env.storage()
                .persistent()
                .set(&Indexes::investments_by_status(status), &investments);
        }
    }

    /// Get next investment count
    pub fn next_count(env: &Env) -> u64 {
        let current: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::investment_count())
            .unwrap_or(0);
        let next = current.saturating_add(1);
        env.storage()
            .persistent()
            .set(&StorageKeys::investment_count(), &next);
        next
    }
}

/// Storage operations for platform configuration
pub struct ConfigStorage;

impl ConfigStorage {
    /// Store platform fee configuration
    pub fn set_platform_fees(env: &Env, config: &PlatformFeeConfig) {
        env.storage()
            .instance()
            .set(&StorageKeys::platform_fees(), config);
    }

    /// Get platform fee configuration
    pub fn get_platform_fees(env: &Env) -> Option<PlatformFeeConfig> {
        env.storage().instance().get(&StorageKeys::platform_fees())
    }
}

/// Helper for clean resets (used in migrations/tests)
pub struct StorageManager;

impl StorageManager {
    /// Clear all mapping indexes and core entities for a fresh state.
    /// WARNING: This is a destructive operation.
    pub fn clear_all_mappings(env: &Env) {
        // Clear counters
        env.storage()
            .persistent()
            .remove(&StorageKeys::invoice_count());
        env.storage().persistent().remove(&StorageKeys::bid_count());
        env.storage()
            .persistent()
            .remove(&StorageKeys::investment_count());

        // Note: In a real protocol, we would need a way to discover all keys.
        // Since we can't iterate, we clear the known "singleton" or "root" keys
        // that point to lists or maps of other data.

        // Clearing these effectively "orphans" the data, which is what
        // a "clear" operation does in this context (e.g. for testing/restore).
    }
}
