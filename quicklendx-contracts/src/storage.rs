//! Storage management for the QuickLendX invoice factoring protocol.
//!
//! This module defines storage keys, indexing strategies, and storage operations
//! for efficient data retrieval and management.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec};

use crate::types::{Bid, BidStatus, Investment, InvestmentStatus, Invoice, InvoiceCategory, InvoiceStatus, PlatformFeeConfig};

/// Storage keys for the contract
pub struct StorageKeys;

/// Primary storage key namespace for core entities.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Invoice(BytesN<32>),
    Bid(BytesN<32>),
    Investment(BytesN<32>),
}

impl StorageKeys {
    pub fn platform_fees() -> Symbol { symbol_short!("fees") }
    pub fn invoice_count() -> Symbol { symbol_short!("inv_count") }
    pub fn bid_count() -> Symbol { symbol_short!("bid_count") }
    pub fn investment_count() -> Symbol { symbol_short!("inv_cnt") }
}

/// Secondary indexes for efficient querying
pub struct Indexes;

impl Indexes {
    pub fn invoices_by_business(business: &Address) -> (Symbol, Address) {
        (symbol_short!("inv_bus"), business.clone())
    }

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

    pub fn bids_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("bids_inv"), invoice_id.clone())
    }

    pub fn bids_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("bids_invr"), investor.clone())
    }

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

    pub fn investments_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("invst_inv"), invoice_id.clone())
    }

    pub fn investments_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("inv_invst"), investor.clone())
    }

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

    pub fn invoices_by_customer(customer_name: &String) -> (Symbol, String) {
        (symbol_short!("inv_cust"), customer_name.clone())
    }

    pub fn invoices_by_tax_id(tax_id: &String) -> (Symbol, String) {
        (symbol_short!("inv_taxid"), tax_id.clone())
    }

    pub fn invoices_by_tag(tag: &String) -> (Symbol, String) {
        (symbol_short!("inv_tag"), tag.clone())
    }

    pub fn invoices_by_category(category: InvoiceCategory) -> (Symbol, Symbol) {
        let cat_symbol = match category {
            InvoiceCategory::Services => symbol_short!("services"),
            InvoiceCategory::Goods => symbol_short!("goods"),
            InvoiceCategory::Consulting => symbol_short!("consult"),
            InvoiceCategory::Logistics => symbol_short!("logist"),
            InvoiceCategory::Other => symbol_short!("other"),
        };
        (symbol_short!("inv_cat"), cat_symbol)
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

    pub fn store_invoice(env: &Env, invoice: &Invoice) {
        Self::store(env, invoice)
    }

    pub fn get_by_business(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_business(business);
        env.storage().persistent().get(&key).unwrap_or(Vec::new(env))
    }

    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        Self::get_by_business(env, business)
    }

    pub fn get_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_status(status);
        env.storage().persistent().get(&key).unwrap_or(Vec::new(env))
    }

    pub fn get_invoices_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        Self::get_by_status(env, status)
    }

    pub fn get(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        env.storage().persistent().get(&DataKey::Invoice(invoice_id.clone()))
    }

    pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        Self::get(env, invoice_id)
    }

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

    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        Self::update(env, invoice)
    }

    pub fn next_count(env: &Env) -> u64 {
        let current: u64 = env.storage().persistent().get(&StorageKeys::invoice_count()).unwrap_or(0);
        let next = current.saturating_add(1);
        env.storage().persistent().set(&StorageKeys::invoice_count(), &next);
        next
    }

    pub fn get_total_count(env: &Env) -> u64 {
        env.storage().persistent().get(&StorageKeys::invoice_count()).unwrap_or(0)
    }

    pub fn delete_invoice(env: &Env, invoice_id: &BytesN<32>) {
        if let Some(invoice) = Self::get(env, invoice_id) {
            Self::remove_from_status_index(env, invoice.status, invoice_id);
            Self::remove_from_business_index(env, &invoice.business, invoice_id);
            if let Some(ref name) = invoice.metadata_customer_name {
                Self::remove_from_customer_index(env, name, invoice_id);
            }
            if let Some(ref tax_id) = invoice.metadata_tax_id {
                Self::remove_from_tax_id_index(env, tax_id, invoice_id);
            }
        }
        env.storage().persistent().remove(&DataKey::Invoice(invoice_id.clone()));
    }

    pub fn clear_all(env: &Env) {
        StorageManager::clear_all_mappings(env);
    }

    pub fn get_invoices_with_rating_above(env: &Env, threshold: u32) -> Vec<BytesN<32>> {
        let mut matches = Vec::new(env);
        for invoice_id in Self::get_all_invoice_ids(env).iter() {
            if let Some(invoice) = Self::get(env, &invoice_id) {
                if invoice.average_rating.map_or(false, |rating| rating > threshold) {
                    matches.push_back(invoice_id);
                }
            }
        }
        matches
    }

    pub fn get_invoices_with_ratings_count(env: &Env) -> u32 {
        let mut count = 0u32;
        for invoice_id in Self::get_all_invoice_ids(env).iter() {
            if let Some(invoice) = Self::get(env, &invoice_id) {
                if invoice.total_ratings > 0 {
                    count = count.saturating_add(1);
                }
            }
        }
        count
    }

    pub fn add_to_status_invoices(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        Self::add_to_status_index(env, status, invoice_id);
    }

    pub fn remove_from_status_invoices(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        Self::remove_from_status_index(env, status, invoice_id);
    }

    pub fn get_invoices_by_category_and_status(env: &Env, category: crate::invoice::InvoiceCategory, status: InvoiceStatus) -> Vec<BytesN<32>> {
        let mut matches = Vec::new(env);
        for invoice_id in Self::get_by_status(env, status).iter() {
            if let Some(invoice) = Self::get(env, &invoice_id) {
                if invoice.category == category {
                    matches.push_back(invoice_id);
                }
            }
        }
        matches
    }

    fn add_to_business_index(env: &Env, business: &Address, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_business(env, business);
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage().persistent().set(&Indexes::invoices_by_business(business), &invoices);
        }
    }

    fn remove_from_business_index(env: &Env, business: &Address, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_business(env, business);
        if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
            invoices.remove(pos as u32);
            env.storage().persistent().set(&Indexes::invoices_by_business(business), &invoices);
        }
    }

    fn add_to_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_status(env, status.clone());
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage().persistent().set(&Indexes::invoices_by_status(status), &invoices);
        }
    }

    fn remove_from_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_status(env, status.clone());
        if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
            invoices.remove(pos as u32);
            env.storage().persistent().set(&Indexes::invoices_by_status(status), &invoices);
        }
    }

    pub fn add_to_customer_index(env: &Env, customer_name: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_customer(customer_name);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_from_customer_index(env: &Env, customer_name: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_customer(customer_name);
        let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        let mut filtered = Vec::new(env);
        for id in ids.iter() {
            if id != *invoice_id { filtered.push_back(id.clone()); }
        }
        env.storage().persistent().set(&key, &filtered);
    }

    pub fn add_to_tax_id_index(env: &Env, tax_id: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tax_id(tax_id);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_from_tax_id_index(env: &Env, tax_id: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tax_id(tax_id);
        let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        let mut filtered = Vec::new(env);
        for id in ids.iter() {
            if id != *invoice_id { filtered.push_back(id.clone()); }
        }
        env.storage().persistent().set(&key, &filtered);
    }

    pub fn add_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tag(tag);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tag(tag);
        let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        let mut filtered = Vec::new(env);
        for id in ids.iter() {
            if id != *invoice_id { filtered.push_back(id.clone()); }
        }
        env.storage().persistent().set(&key, &filtered);
    }

    pub fn add_category_index(env: &Env, category: InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_category(category);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_category_index(env: &Env, category: InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_category(category);
        let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        let mut filtered = Vec::new(env);
        for id in ids.iter() {
            if id != *invoice_id { filtered.push_back(id.clone()); }
        }
        env.storage().persistent().set(&key, &filtered);
    }

    pub fn get_invoices_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_customer(customer_name)).unwrap_or(Vec::new(env))
    }

    pub fn get_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>> {
        Self::get_invoices_by_customer(env, customer_name)
    }

    pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_tax_id(tax_id)).unwrap_or(Vec::new(env))
    }

    pub fn get_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        Self::get_invoices_by_tax_id(env, tax_id)
    }

    pub fn get_invoices_by_category(env: &Env, category: &crate::invoice::InvoiceCategory) -> Vec<BytesN<32>> {
        let mut matches = Vec::new(env);
        for invoice_id in Self::get_all_invoice_ids(env).iter() {
            if let Some(invoice) = Self::get(env, &invoice_id) {
                if invoice.category == *category { matches.push_back(invoice_id); }
            }
        }
        matches
    }

    pub fn count_active_business_invoices(env: &Env, business: &Address) -> u32 {
        let mut count = 0u32;
        for invoice_id in Self::get_by_business(env, business).iter() {
            if let Some(invoice) = Self::get(env, &invoice_id) {
                if crate::protocol_limits::is_active_status(&invoice.status) {
                    count = count.saturating_add(1);
                }
            }
        }
        count
    }

    pub fn get_invoices_by_tag(env: &Env, tag: &String) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_tag(tag)).unwrap_or(Vec::new(env))
    }

    pub fn get_invoices_by_tags(env: &Env, tags: &Vec<String>) -> Vec<BytesN<32>> {
        if tags.is_empty() { return Vec::new(env); }
        let mut result = Vec::new(env);
        let first_tag = tags.get(0).unwrap();
        let first_ids = Self::get_invoices_by_tag(env, &first_tag);
        
        for id in first_ids.iter() {
            let mut all_match = true;
            for i in 1..tags.len() {
                let tag = tags.get(i).unwrap();
                let tag_ids = Self::get_invoices_by_tag(env, &tag);
                if !tag_ids.contains(&id) {
                    all_match = false;
                    break;
                }
            }
            if all_match { result.push_back(id); }
        }
        result
    }

    pub fn get_invoice_count_by_tag(env: &Env, tag: &String) -> u32 {
        Self::get_invoices_by_tag(env, tag).len()
    }

    pub fn add_metadata_indexes(env: &Env, invoice: &Invoice) {
        if let Some(ref name) = invoice.metadata_customer_name {
            Self::add_to_customer_index(env, name, &invoice.id);
        }
        if let Some(ref tax_id) = invoice.metadata_tax_id {
            Self::add_to_tax_id_index(env, tax_id, &invoice.id);
        }
    }

    pub fn remove_metadata_indexes(env: &Env, metadata: &crate::types::InvoiceMetadata, invoice_id: &BytesN<32>) {
        Self::remove_from_customer_index(env, &metadata.customer_name, invoice_id);
        Self::remove_from_tax_id_index(env, &metadata.tax_id, invoice_id);
    }
}

/// Storage operations for bids
pub use crate::bid::BidStorage;

/// Storage operations for investments
pub use crate::investment::InvestmentStorage;


/// Storage operations for platform configuration
pub struct ConfigStorage;

impl ConfigStorage {
    pub fn set_platform_fees(env: &Env, config: &PlatformFeeConfig) {
        env.storage().instance().set(&StorageKeys::platform_fees(), config);
    }
    pub fn get_platform_fees(env: &Env) -> Option<PlatformFeeConfig> {
        env.storage().instance().get(&StorageKeys::platform_fees())
    }
}

/// Helper for clean resets (used in migrations/tests)
pub struct StorageManager;

impl StorageManager {
    pub fn clear_all_mappings(env: &Env) {
        env.storage().persistent().remove(&StorageKeys::invoice_count());
        env.storage().persistent().remove(&StorageKeys::bid_count());
        env.storage().persistent().remove(&StorageKeys::investment_count());
    }
}
