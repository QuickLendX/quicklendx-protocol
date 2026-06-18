//! Storage management for the QuickLendX invoice factoring protocol.
//!
//! This module defines storage keys, indexing strategies, and storage operations
//! for efficient data retrieval and management.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec};

use crate::protocol_limits;
use crate::types::{
    BidStatus, InvestmentStatus, Invoice, InvoiceCategory, InvoiceStatus,
    PlatformFeeConfig, RebuildReport,
};

/// Default TTL threshold for persistent storage (adjust the value as needed)
pub const PERSISTENT_TTL_THRESHOLD: u64 = 34_732_800; // ~30 days at 5s/ledger

pub fn extend_persistent_ttl<T>(env: &Env, key: &T)
where
    T: soroban_sdk::IntoVal<soroban_sdk::Env, soroban_sdk::Val>,
{
    let ttl_u32: u32 = PERSISTENT_TTL_THRESHOLD.try_into().unwrap_or(0);
    env.storage().persistent().extend_ttl(key, ttl_u32, ttl_u32);
}



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
    pub fn platform_fees() -> Symbol {
        symbol_short!("fees")
    }
    pub fn invoice_count() -> Symbol {
        symbol_short!("inv_count")
    }
    pub fn bid_count() -> Symbol {
        symbol_short!("bid_count")
    }
    pub fn investment_count() -> Symbol {
        symbol_short!("inv_cnt")
    }
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
            InvoiceCategory::Products => symbol_short!("products"),
            InvoiceCategory::Manufacturing => symbol_short!("manufac"),
            InvoiceCategory::Technology => symbol_short!("tech"),
            InvoiceCategory::Healthcare => symbol_short!("health"),
            InvoiceCategory::Other => symbol_short!("other"),
        };
        (symbol_short!("inv_cat"), cat_symbol)
    }
}

/// Storage operations for invoices.
/// 
/// ## Invariants Maintained
/// - Each invoice exists in exactly one status index (Pending, Verified, Funded, Paid, 
///   Defaulted, Cancelled, or Refunded).
/// - `get_invoice_count_by_status::<S>().len() == get_total_invoice_count()` for the sum 
///   across all statuses (count-index agreement).
/// - When status changes, removal from old status index and addition to new status index 
///   are performed atomically within the same transaction.
/// - No invoice ID in any status index references a non-existent invoice record.
pub struct InvoiceStorage;

impl InvoiceStorage {
    /// Store an invoice and update all its secondary indexes.
    pub fn store(env: &Env, invoice: &Invoice) {
        let key = DataKey::Invoice(invoice.id.clone());
        env.storage()
            .persistent()
            .set(&key, invoice);
        extend_persistent_ttl(env, &key);
        Self::add_to_business_index(env, &invoice.business, &invoice.id);
        Self::add_to_status_index(env, invoice.status.clone(), &invoice.id);
        if let Some(ref name) = invoice.metadata_customer_name {
            Self::add_to_customer_index(env, name, &invoice.id);
        }
        if let Some(ref tax_id) = invoice.metadata_tax_id {
            Self::add_to_tax_id_index(env, tax_id, &invoice.id);
        }
        Self::add_category_index(env, &invoice.category, &invoice.id);
        for tag in invoice.tags.iter() {
            Self::add_tag_index(env, &tag, &invoice.id);
        }
    }

    pub fn store_invoice(env: &Env, invoice: &Invoice) {
        Self::store(env, invoice)
    }

    pub fn get_by_business(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_business(business);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env))
    }

    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        Self::get_by_business(env, business)
    }

    pub fn get_all_categories(env: &Env) -> Vec<InvoiceCategory> {
        let mut categories = Vec::new(env);
        categories.push_back(InvoiceCategory::Goods);
        categories.push_back(InvoiceCategory::Logistics);
        categories.push_back(InvoiceCategory::Services);
        categories.push_back(InvoiceCategory::Products);
        categories.push_back(InvoiceCategory::Consulting);
        categories.push_back(InvoiceCategory::Manufacturing);
        categories.push_back(InvoiceCategory::Technology);
        categories.push_back(InvoiceCategory::Healthcare);
        categories.push_back(InvoiceCategory::Other);
        categories
    }

    pub fn get_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        let key = Indexes::invoices_by_status(status);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env))
    }

    pub fn get_invoices_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        Self::get_by_status(env, status)
    }

    pub fn get(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        let key = DataKey::Invoice(invoice_id.clone());
        let result = env.storage().persistent().get(&key);
        if result.is_some() {
            extend_persistent_ttl(env, &key);
        }
        result
    }

    pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        Self::get(env, invoice_id)
    }

    pub fn update(env: &Env, invoice: &Invoice) {
        if let Some(old) = Self::get(env, &invoice.id) {
            if old.status != invoice.status {
                Self::remove_from_status_index(env, old.status, &invoice.id);
                Self::add_to_status_index(env, invoice.status.clone(), &invoice.id);
            }
            if old.metadata_customer_name != invoice.metadata_customer_name {
                if let Some(ref name) = old.metadata_customer_name {
                    Self::remove_from_customer_index(env, name, &invoice.id);
                }
                if let Some(ref name) = invoice.metadata_customer_name {
                    Self::add_to_customer_index(env, name, &invoice.id);
                }
            }
            if old.metadata_tax_id != invoice.metadata_tax_id {
                if let Some(ref tax_id) = old.metadata_tax_id {
                    Self::remove_from_tax_id_index(env, tax_id, &invoice.id);
                }
                if let Some(ref tax_id) = invoice.metadata_tax_id {
                    Self::add_to_tax_id_index(env, tax_id, &invoice.id);
                }
            }
            if old.category != invoice.category {
                Self::remove_category_index(env, &old.category, &invoice.id);
                Self::add_category_index(env, &invoice.category, &invoice.id);
            }
            if old.tags != invoice.tags {
                for tag in old.tags.iter() {
                    Self::remove_tag_index(env, &tag, &invoice.id);
                }
                for tag in invoice.tags.iter() {
                    Self::add_tag_index(env, &tag, &invoice.id);
                }
            }
        }
        let key = DataKey::Invoice(invoice.id.clone());
        env.storage()
            .persistent()
            .set(&key, invoice);
        extend_persistent_ttl(env, &key);
    }

    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        Self::update(env, invoice)
    }

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

    pub fn get_total_count(env: &Env) -> u64 {
        env.storage()
            .persistent()
            .get(&StorageKeys::invoice_count())
            .unwrap_or(0)
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
            Self::remove_category_index(env, &invoice.category, invoice_id);
            for tag in invoice.tags.iter() {
                Self::remove_tag_index(env, &tag, invoice_id);
            }
        }
        env.storage()
            .persistent()
            .remove(&DataKey::Invoice(invoice_id.clone()));
    }

    pub fn clear_all(env: &Env) {
        let ids = Self::get_all_invoice_ids(env);
        for id in ids.iter() {
            Self::delete_invoice(env, &id);
        }
        StorageManager::clear_all_mappings(env);
    }

    pub fn get_all_invoice_ids(env: &Env) -> Vec<BytesN<32>> {
        let mut all = Vec::new(env);
        let mut statuses = Vec::new(env);
        statuses.push_back(InvoiceStatus::Pending);
        statuses.push_back(InvoiceStatus::Verified);
        statuses.push_back(InvoiceStatus::Funded);
        statuses.push_back(InvoiceStatus::Paid);
        statuses.push_back(InvoiceStatus::Defaulted);
        statuses.push_back(InvoiceStatus::Cancelled);
        statuses.push_back(InvoiceStatus::Refunded);

        for status in statuses.iter() {
            for id in Self::get_by_status(env, status).iter() {
                if !all.contains(&id) {
                    all.push_back(id);
                }
            }
        }
        all
    }

    pub fn get_invoices_with_rating_above(env: &Env, threshold: u32) -> Vec<BytesN<32>> {
        let mut matches = Vec::new(env);
        for invoice_id in Self::get_all_invoice_ids(env).iter() {
            if let Some(invoice) = Self::get(env, &invoice_id) {
                if invoice
                    .average_rating
                    .map_or(false, |rating| rating > threshold)
                {
                    matches.push_back(invoice_id);
                }
            }
        }
        matches
    }

    pub fn add_to_status_invoices(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        Self::add_to_status_index(env, status, invoice_id);
    }

    pub fn remove_from_status_invoices(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        Self::remove_from_status_index(env, status, invoice_id);
    }

    pub fn get_invoices_by_category_and_status(
        env: &Env,
        category: crate::types::InvoiceCategory,
        status: InvoiceStatus,
    ) -> Vec<BytesN<32>> {
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
            let key = Indexes::invoices_by_business(business);
            env.storage()
                .persistent()
                .set(&key, &invoices);
            extend_persistent_ttl(env, &key);
        }
    }

    fn remove_from_business_index(env: &Env, business: &Address, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_business(env, business);
        if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
            invoices.remove(pos as u32);
            let key = Indexes::invoices_by_business(business);
            env.storage()
                .persistent()
                .set(&key, &invoices);
            extend_persistent_ttl(env, &key);
        }
    }

    fn add_to_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_status(env, status.clone());
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            let key = Indexes::invoices_by_status(status);
            env.storage()
                .persistent()
                .set(&key, &invoices);
            extend_persistent_ttl(env, &key);
        }
    }

    fn remove_from_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let mut invoices = Self::get_by_status(env, status.clone());
        if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
            invoices.remove(pos as u32);
            let key = Indexes::invoices_by_status(status);
            env.storage()
                .persistent()
                .set(&key, &invoices);
            extend_persistent_ttl(env, &key);
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
            extend_persistent_ttl(env, &key);
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
        extend_persistent_ttl(env, &key);
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
            extend_persistent_ttl(env, &key);
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
        extend_persistent_ttl(env, &key);
    }
    pub fn add_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tag(tag);
        let mut ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
            extend_persistent_ttl(env, &key);
        }
    }

    pub fn remove_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tag(tag);
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
        extend_persistent_ttl(env, &key);
    }

    pub fn add_category_index(env: &Env, category: &InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_category(category.clone());
        let mut ids: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &ids);
            extend_persistent_ttl(env, &key);
        }
    }

    pub fn remove_category_index(env: &Env, category: &InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_category(category.clone());
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
        extend_persistent_ttl(env, &key);
    }

    pub fn get_invoices_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_customer(customer_name))
            .unwrap_or(Vec::new(env))
    }

    pub fn get_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>> {
        Self::get_invoices_by_customer(env, customer_name)
    }

    pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_tax_id(tax_id))
            .unwrap_or(Vec::new(env))
    }

    pub fn get_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        Self::get_invoices_by_tax_id(env, tax_id)
    }

    pub fn get_invoices_by_category(
        env: &Env,
        category: &crate::types::InvoiceCategory,
    ) -> Vec<BytesN<32>> {
        let mut matches = Vec::new(env);
        for invoice_id in Self::get_all_invoice_ids(env).iter() {
            if let Some(invoice) = Self::get(env, &invoice_id) {
                if invoice.category == *category {
                    matches.push_back(invoice_id);
                }
            }
        }
        matches
    }

    pub fn get_by_tag(env: &Env, tag: &String) -> Vec<BytesN<32>> {
        Self::get_invoices_by_tag(env, tag)
    }

    pub fn get_by_category(env: &Env, category: InvoiceCategory) -> Vec<BytesN<32>> {
        Self::get_invoices_by_category(env, &category)
    }

    pub fn get_invoice_count_by_category(env: &Env, category: &InvoiceCategory) -> u32 {
        Self::get_invoices_by_category(env, category).len()
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
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_tag(tag))
            .unwrap_or(Vec::new(env))
    }

    pub fn get_invoices_by_tags(env: &Env, tags: &Vec<String>) -> Vec<BytesN<32>> {
        if tags.is_empty() {
            return Vec::new(env);
        }
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
            if all_match {
                result.push_back(id);
            }
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

    pub fn remove_metadata_indexes(
        env: &Env,
        metadata: &crate::types::InvoiceMetadata,
        invoice_id: &BytesN<32>,
    ) {
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
        env.storage()
            .instance()
            .set(&StorageKeys::platform_fees(), config);
    }
    pub fn get_platform_fees(env: &Env) -> Option<PlatformFeeConfig> {
        env.storage().instance().get(&StorageKeys::platform_fees())
    }
}

pub struct StorageManager;
impl StorageManager {
    pub fn clear_all_mappings(env: &Env) {
        env.storage()
            .persistent()
            .remove(&StorageKeys::invoice_count());
        env.storage().persistent().remove(&StorageKeys::bid_count());
        env.storage()
            .persistent()
            .remove(&StorageKeys::investment_count());
    }
}

/// Comprehensive integrity audit for protocol storage indexes.
///
/// This helper provides deep inspection of secondary indexes to ensure no
/// orphan IDs exist and that all records are mutually consistent across
/// different indexing strategies (status, owner, metadata).
pub struct StorageIntegrityAudit;

impl StorageIntegrityAudit {
    /// Audits all invoice-related indexes for consistency and orphans.
    pub fn audit_invoice_integrity(env: &Env) -> Result<(), Vec<String>> {
        let mut errors = Vec::new(env);
        let mut discovered_ids = Vec::new(env);

        // 1. Status Index Audit (Pending, Verified, Funded, Paid, Defaulted, Cancelled, Refunded)
        let statuses = Vec::from_array(
            env,
            [
                InvoiceStatus::Pending,
                InvoiceStatus::Verified,
                InvoiceStatus::Funded,
                InvoiceStatus::Paid,
                InvoiceStatus::Defaulted,
                InvoiceStatus::Cancelled,
                InvoiceStatus::Refunded,
            ],
        );

        for status in statuses.iter() {
            let ids = InvoiceStorage::get_by_status(env, status.clone());
            for id in ids.iter() {
                if !discovered_ids.contains(&id) {
                    discovered_ids.push_back(id.clone());
                }
                match InvoiceStorage::get(env, &id) {
                    None => {
                        errors.push_back(String::from_str(
                            env,
                            "Orphan invoice ID found in status index",
                        ));
                    }
                    Some(invoice) => {
                        if invoice.status != status {
                            errors.push_back(String::from_str(
                                env,
                                "Invoice status mismatch: record vs index",
                            ));
                        }
                    }
                }
            }
        }

        // 2. Cross-Consistency Check for all discovered invoices
        for id in discovered_ids.iter() {
            if let Some(invoice) = InvoiceStorage::get(env, &id) {
                // Check business index
                let business_ids = InvoiceStorage::get_by_business(env, &invoice.business);
                if !business_ids.contains(&id) {
                    errors.push_back(String::from_str(env, "Invoice missing from business index"));
                }

                // Check metadata indexes if present
                if let Some(ref name) = invoice.metadata_customer_name {
                    let customer_ids = InvoiceStorage::get_by_customer(env, name);
                    if !customer_ids.contains(&id) {
                        errors.push_back(String::from_str(
                            env,
                            "Invoice missing from customer metadata index",
                        ));
                    }
                }
                if let Some(ref tax_id) = invoice.metadata_tax_id {
                    let tax_ids = InvoiceStorage::get_by_tax_id(env, tax_id);
                    if !tax_ids.contains(&id) {
                        errors.push_back(String::from_str(
                            env,
                            "Invoice missing from tax ID metadata index",
                        ));
                    }
                }

                // Check tag indexes
                for tag in invoice.tags.iter() {
                    let tag_ids = InvoiceStorage::get_by_tag(env, &tag);
                    if !tag_ids.contains(&id) {
                        errors.push_back(String::from_str(env, "Invoice missing from tag index"));
                    }
                }

                // Check category index
                let category_ids = InvoiceStorage::get_by_category(env, invoice.category.clone());
                if !category_ids.contains(&id) {
                    errors.push_back(String::from_str(env, "Invoice missing from category index"));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Audits all bid-related indexes using the global bid list as source of truth.
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

    /// Performs a full protocol-wide integrity audit.
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

// ============================================================================
// Index Rebuild
// ============================================================================

impl InvoiceStorage {
    /// Recompute secondary indexes for a page of invoices from their canonical records.
    ///
    /// # Why this exists
    /// Secondary indexes (`invoices_by_customer`, `invoices_by_tax_id`,
    /// `invoices_by_tag`, `invoices_by_category`) are denormalized state that can
    /// drift after a backup restore, a partial migration, or a past bug. This
    /// function rebuilds them from the source-of-truth `Invoice` records.
    ///
    /// # Idempotency
    /// Every index write is a deduplication-guarded append (`add_to_*_index`
    /// checks for duplicates before inserting). Calling this function twice over
    /// the same range leaves the indexes in the same state as calling it once.
    ///
    /// # Resumability
    /// `offset` and `limit` work against the list returned by
    /// `get_all_invoice_ids`, which is stable within a ledger. Pass
    /// `report.next_offset` as `offset` on the next call. Stop when
    /// `report.next_offset == report.scanned` (last page reached).
    ///
    /// # Arguments
    /// * `env`    - Contract environment.
    /// * `offset` - Zero-based starting position in the full invoice ID list.
    /// * `limit`  - Max invoices to process; capped at `MAX_REBUILD_PAGE`.
    pub fn rebuild_indexes_page(env: &Env, offset: u32, limit: u32) -> RebuildReport {
        const MAX_REBUILD_PAGE: u32 = 100;
        let capped = if limit > MAX_REBUILD_PAGE { MAX_REBUILD_PAGE } else { limit };

        let all_ids = Self::get_all_invoice_ids(env);
        let total = all_ids.len() as u32;

        let start = offset.min(total);
        let end = start.saturating_add(capped).min(total);

        let mut reindexed: u32 = 0;
        let mut i = start;
        while i < end {
            if let Some(id) = all_ids.get(i) {
                if let Some(invoice) = Self::get(env, &id) {
                    // customer name
                    if let Some(ref name) = invoice.metadata_customer_name {
                        Self::add_to_customer_index(env, name, &invoice.id);
                    }
                    // tax id
                    if let Some(ref tax_id) = invoice.metadata_tax_id {
                        Self::add_to_tax_id_index(env, tax_id, &invoice.id);
                    }
                    // tags
                    for tag in invoice.tags.iter() {
                        Self::add_tag_index(env, &tag, &invoice.id);
                    }
                    // category
                    Self::add_category_index(env, &invoice.category, &invoice.id);

                    reindexed = reindexed.saturating_add(1);
                }
            }
            i = i.saturating_add(1);
        }

        RebuildReport {
            scanned: end.saturating_sub(start),
            reindexed,
            next_offset: end,
        }
    }
}
