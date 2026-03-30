//! Storage management for the QuickLendX invoice factoring protocol.
//!
//! This module defines storage keys, indexing strategies, and storage operations
//! for efficient data retrieval and management.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec};

use crate::types::{
    Bid, Investment, Invoice, InvoiceCategory, InvoiceStatus, PlatformFeeConfig,
};

// ---------------------------------------------------------------------------
// Keys
// ---------------------------------------------------------------------------

/// Primary storage key namespace for core entities.
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    /// Primary key for an Invoice record. Keyed by invoice ID.
    Invoice(BytesN<32>),
    /// Primary key for a Bid record. Keyed by bid ID.
    Bid(BytesN<32>),
    /// Primary key for an Investment record. Keyed by investment ID.
    Investment(BytesN<32>),
    /// Secondary index for Customer Name metadata
    MetadataCustomer(String),
    /// Secondary index for Tax ID metadata
    MetadataTax(String),
}

pub struct StorageKeys;

impl StorageKeys {
    pub fn platform_fees() -> Symbol {
        symbol_short!("plt_fee")
    }
    pub fn invoice_count() -> Symbol {
        symbol_short!("inv_cnt")
    }
    pub fn bid_count() -> Symbol {
        symbol_short!("bid_cnt")
    }
    pub fn investment_count() -> Symbol {
        symbol_short!("invstcnt")
    }
    pub fn dispute_index() -> Symbol {
        symbol_short!("disp_idx")
    }
}

pub struct Indexes;

impl Indexes {
    pub fn invoices_by_business(business: &Address) -> (Symbol, Address) {
        (symbol_short!("inv_bus"), business.clone())
    }
    pub fn invoices_by_status(status: &InvoiceStatus) -> (Symbol, Symbol) {
        let sym = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("default"),
            InvoiceStatus::Cancelled => symbol_short!("cancel"),
            InvoiceStatus::Refunded => symbol_short!("refund"),
        };
        (symbol_short!("inv_stat"), sym)
    }
    pub fn invoices_by_category(category: &InvoiceCategory) -> (Symbol, InvoiceCategory) {
        (symbol_short!("inv_cat"), category.clone())
    }
    pub fn invoices_by_tag(tag: &String) -> (Symbol, String) {
        (symbol_short!("inv_tag"), tag.clone())
    }
    pub fn invoices_by_customer(name: &String) -> (Symbol, String) {
        (symbol_short!("inv_cust"), name.clone())
    }
    pub fn invoices_by_tax_id(tax_id: &String) -> (Symbol, String) {
        (symbol_short!("inv_tax"), tax_id.clone())
    }
    pub fn bids_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("bid_inv"), invoice_id.clone())
    }
    pub fn bids_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("bid_invr"), investor.clone())
    }
    pub fn investments_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (symbol_short!("invt_inv"), invoice_id.clone())
    }
    pub fn investments_by_investor(investor: &Address) -> (Symbol, Address) {
        (symbol_short!("invt_inr"), investor.clone())
    }
}

// ---------------------------------------------------------------------------
// Invoice Storage
// ---------------------------------------------------------------------------

pub struct InvoiceStorage;

impl InvoiceStorage {
    pub fn metadata_customer_key(name: &String) -> DataKey {
        DataKey::MetadataCustomer(name.clone())
    }

    pub fn metadata_tax_key(tax_id: &String) -> DataKey {
        DataKey::MetadataTax(tax_id.clone())
    }

    pub fn get_invoice_count_by_category(env: &Env, category: &InvoiceCategory) -> u32 {
        Self::get_invoices_by_category(env, category).len()
    }

    pub fn get_invoice_count_by_tag(env: &Env, tag: &String) -> u32 {
        Self::get_invoices_by_tag(env, tag).len()
    }

    pub fn store_invoice(env: &Env, invoice: &Invoice) {
        let is_new = !env.storage().persistent().has(&DataKey::Invoice(invoice.id.clone()));
        env.storage().persistent().set(&DataKey::Invoice(invoice.id.clone()), invoice);

        if is_new {
            let mut count: u32 = env.storage().instance().get(&StorageKeys::invoice_count()).unwrap_or(0);
            count = count.saturating_add(1);
            env.storage().instance().set(&StorageKeys::invoice_count(), &count);

            Self::add_to_business_index(env, &invoice.business, &invoice.id);
        }

        Self::update_indexes(env, invoice);
    }

    pub fn get_invoice(env: &Env, id: &BytesN<32>) -> Option<Invoice> {
        env.storage().persistent().get(&DataKey::Invoice(id.clone()))
    }

    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        if let Some(old) = Self::get_invoice(env, &invoice.id) {
            if old.status != invoice.status {
                Self::remove_from_status_index(env, &old.status, &invoice.id);
                Self::add_to_status_index(env, &invoice.status, &invoice.id);
            }
            // Update other indexes as needed...
            Self::update_indexes(env, invoice);
        }
        env.storage().persistent().set(&DataKey::Invoice(invoice.id.clone()), invoice);
    }

    fn update_indexes(env: &Env, invoice: &Invoice) {
        Self::add_to_status_index(env, &invoice.status, &invoice.id);
        Self::add_to_category_index(env, &invoice.category, &invoice.id);
        for tag in invoice.tags.iter() {
            Self::add_to_tag_index(env, &tag, &invoice.id);
        }
        if let Some(ref name) = invoice.metadata_customer_name {
            Self::add_to_customer_index(env, name, &invoice.id);
        }
        if let Some(ref tax) = invoice.metadata_tax_id {
            Self::add_to_tax_id_index(env, tax, &invoice.id);
        }
    }

    // Index helpers
    pub fn add_to_status_index(env: &Env, status: &InvoiceStatus, id: &BytesN<32>) {
        let key = Indexes::invoices_by_status(status);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_from_status_index(env: &Env, status: &InvoiceStatus, id: &BytesN<32>) {
        let key = Indexes::invoices_by_status(status);
        if let Some(ids) = env.storage().persistent().get::<_, Vec<BytesN<32>>>(&key) {
            let mut updated = Vec::new(env);
            for existing in ids.iter() {
                if existing != *id {
                    updated.push_back(existing);
                }
            }
            env.storage().persistent().set(&key, &updated);
        }
    }

    pub fn add_to_business_index(env: &Env, business: &Address, id: &BytesN<32>) {
        let key = Indexes::invoices_by_business(business);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn add_to_category_index(env: &Env, category: &InvoiceCategory, id: &BytesN<32>) {
        let key = Indexes::invoices_by_category(category);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn add_to_tag_index(env: &Env, tag: &String, id: &BytesN<32>) {
        let key = Indexes::invoices_by_tag(tag);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn add_to_customer_index(env: &Env, name: &String, id: &BytesN<32>) {
        let key = Indexes::invoices_by_customer(name);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn add_to_tax_id_index(env: &Env, tax_id: &String, id: &BytesN<32>) {
        let key = Indexes::invoices_by_tax_id(tax_id);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn get_invoices_by_status(env: &Env, status: &InvoiceStatus) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_status(status)).unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_business(business)).unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_total_invoice_count(env: &Env) -> u32 {
        env.storage().instance().get(&StorageKeys::invoice_count()).unwrap_or(0)
    }

    pub fn get_invoices_by_category(env: &Env, category: &InvoiceCategory) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_category(category);
        let invoices: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        let mut result = Vec::new(env);
        for id in invoices.iter() {
            if let Some(invoice) = Self::get_invoice(env, &id) {
                if invoice.status == InvoiceStatus::Verified {
                    result.push_back(id);
                }
            }
        }
        result
    }

    pub fn get_invoices_by_tag(env: &Env, tag: &String) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_tag(tag);
        let invoices: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        let mut result = Vec::new(env);
        for id in invoices.iter() {
            if let Some(invoice) = Self::get_invoice(env, &id) {
                if invoice.status == InvoiceStatus::Verified {
                    result.push_back(id);
                }
            }
        }
        result
    }

    pub fn get_all_categories(env: &Env) -> Vec<InvoiceCategory> {
        let mut categories = Vec::new(env);
        categories.push_back(InvoiceCategory::Services);
        categories.push_back(InvoiceCategory::Products);
        categories.push_back(InvoiceCategory::Consulting);
        categories.push_back(InvoiceCategory::Manufacturing);
        categories.push_back(InvoiceCategory::Technology);
        categories.push_back(InvoiceCategory::Healthcare);
        categories.push_back(InvoiceCategory::Other);
        categories
    }
}

// ---------------------------------------------------------------------------
// Bid Storage
// ---------------------------------------------------------------------------

pub struct BidStorage;

impl BidStorage {
    pub fn store_bid(env: &Env, bid: &Bid) {
        env.storage().persistent().set(&DataKey::Bid(bid.bid_id.clone()), bid);
        Self::add_to_invoice_index(env, &bid.invoice_id, &bid.bid_id);
        Self::add_to_investor_index(env, &bid.investor, &bid.bid_id);
    }

    pub fn get_bid(env: &Env, id: &BytesN<32>) -> Option<Bid> {
        env.storage().persistent().get(&DataKey::Bid(id.clone()))
    }

    pub fn update_bid(env: &Env, bid: &Bid) {
        env.storage().persistent().set(&DataKey::Bid(bid.bid_id.clone()), bid);
    }

    fn add_to_invoice_index(env: &Env, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        let key = Indexes::bids_by_invoice(invoice_id);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *bid_id) {
            ids.push_back(bid_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    fn add_to_investor_index(env: &Env, investor: &Address, bid_id: &BytesN<32>) {
        let key = Indexes::bids_by_investor(investor);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *bid_id) {
            ids.push_back(bid_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn get_bids_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::bids_by_invoice(invoice_id)).unwrap_or_else(|| Vec::new(env))
    }
}

// ---------------------------------------------------------------------------
// Investment Storage
// ---------------------------------------------------------------------------

pub struct InvestmentStorage;

impl InvestmentStorage {
    pub fn store_investment(env: &Env, investment: &Investment) {
        env.storage().persistent().set(&DataKey::Investment(investment.investment_id.clone()), investment);
        Self::add_to_invoice_index(env, &investment.invoice_id, &investment.investment_id);
        Self::add_to_investor_index(env, &investment.investor, &investment.investment_id);
    }

    pub fn get_investment(env: &Env, id: &BytesN<32>) -> Option<Investment> {
        env.storage().persistent().get(&DataKey::Investment(id.clone()))
    }

    pub fn update_investment(env: &Env, investment: &Investment) {
        env.storage().persistent().set(&DataKey::Investment(investment.investment_id.clone()), investment);
    }

    fn add_to_invoice_index(env: &Env, invoice_id: &BytesN<32>, inv_id: &BytesN<32>) {
        let key = Indexes::investments_by_invoice(invoice_id);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *inv_id) {
            ids.push_back(inv_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    fn add_to_investor_index(env: &Env, investor: &Address, inv_id: &BytesN<32>) {
        let key = Indexes::investments_by_investor(investor);
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *inv_id) {
            ids.push_back(inv_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn get_investment_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Investment> {
        let key = Indexes::investments_by_invoice(invoice_id);
        let ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if let Some(id) = ids.get(0) {
            return Self::get_investment(env, &id);
        }
        None
    }

    pub fn get_investments_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        let key = Indexes::investments_by_investor(investor);
        env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env))
    }
}

// ---------------------------------------------------------------------------
// Dispute Storage
// ---------------------------------------------------------------------------

pub struct DisputeStorage;

impl DisputeStorage {
    pub fn add_to_dispute_index(env: &Env, invoice_id: &BytesN<32>) {
        let key = StorageKeys::dispute_index();
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| existing == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn get_dispute_index(env: &Env) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&StorageKeys::dispute_index()).unwrap_or_else(|| Vec::new(env))
    }
}

// ---------------------------------------------------------------------------
// Config Storage
// ---------------------------------------------------------------------------

pub struct ConfigStorage;

impl ConfigStorage {
    pub fn set_platform_fees(env: &Env, config: &PlatformFeeConfig) {
        env.storage().instance().set(&StorageKeys::platform_fees(), config);
    }
    pub fn get_platform_fees(env: &Env) -> Option<PlatformFeeConfig> {
        env.storage().instance().get(&StorageKeys::platform_fees())
    }
}

pub struct StorageManager;

impl StorageManager {
    pub fn clear_all_mappings(env: &Env) {
        env.storage().instance().remove(&StorageKeys::invoice_count());
        env.storage().instance().remove(&StorageKeys::bid_count());
        env.storage().instance().remove(&StorageKeys::investment_count());
        env.storage().persistent().remove(&StorageKeys::dispute_index());
    }
}
