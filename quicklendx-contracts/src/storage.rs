//! Storage management for the QuickLendX invoice factoring protocol.
//!
//! This module defines storage keys, indexing strategies, and storage operations
//! for efficient data retrieval and management.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec};

use crate::invoice::{Invoice, InvoiceStatus};
use crate::profits::PlatformFeeConfig;
use crate::types::{Bid, BidStatus, Investment, InvestmentStatus};

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
    pub fn invoices_by_business(business: &Address) -> (Symbol, Address) { (symbol_short!("inv_bus"), business.clone()) }
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
    pub fn bids_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) { (symbol_short!("bids_inv"), invoice_id.clone()) }
    pub fn bids_by_investor(investor: &Address) -> (Symbol, Address) { (symbol_short!("bids_invr"), investor.clone()) }
    pub fn bids_by_status(status: BidStatus) -> (Symbol, Symbol) {
        let status_symbol = match status {
            BidStatus::Placed => symbol_short!("placed"),
            BidStatus::Withdrawn => symbol_short!("withdrawn"),
            BidStatus::Accepted => symbol_short!("accepted"),
            BidStatus::Expired => symbol_short!("expired"),
        };
        (symbol_short!("bids_stat"), status_symbol)
    }
    pub fn investments_by_invoice(invoice_id: &BytesN<32>) -> (Symbol, BytesN<32>) { (symbol_short!("invst_inv"), invoice_id.clone()) }
    pub fn investments_by_investor(investor: &Address) -> (Symbol, Address) { (symbol_short!("inv_invst"), investor.clone()) }
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
    pub fn invoices_by_customer(name: &String) -> (Symbol, String) { (symbol_short!("inv_cust"), name.clone()) }
    pub fn invoices_by_tax_id(tax_id: &String) -> (Symbol, String) { (symbol_short!("inv_taxid"), tax_id.clone()) }
    pub fn dispute_index() -> Symbol { symbol_short!("dispute") }
}

/// Storage operations for invoices
pub struct InvoiceStorage;

impl InvoiceStorage {
    /// Store an invoice and update all its secondary indexes.
    pub fn store(env: &Env, invoice: &Invoice) {
        env.storage().persistent().set(&DataKey::Invoice(invoice.id.clone()), invoice);
        Self::add_to_business_index(env, &invoice.business, &invoice.id);
        Self::add_to_status_index(env, invoice.status.clone(), &invoice.id);
        if let Some(ref name) = invoice.metadata_customer_name { Self::add_to_customer_index(env, name, &invoice.id); }
        if let Some(ref tax_id) = invoice.metadata_tax_id { Self::add_to_tax_id_index(env, tax_id, &invoice.id); }
    }

    /// Retrieve an invoice by its unique ID.
    pub fn get(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        env.storage().persistent().get(&DataKey::Invoice(invoice_id.clone()))
    }

    /// Update an existing invoice and maintain index consistency.
    pub fn update(env: &Env, invoice: &Invoice) {
        if let Some(old) = Self::get(env, &invoice.id) {
            if old.status != invoice.status {
                Self::remove_from_status_index(env, old.status, &invoice.id);
                Self::add_to_status_index(env, invoice.status.clone(), &invoice.id);
            }
            if old.metadata_customer_name != invoice.metadata_customer_name {
                if let Some(ref name) = old.metadata_customer_name { Self::remove_from_customer_index(env, name, &invoice.id); }
                if let Some(ref name) = invoice.metadata_customer_name { Self::add_to_customer_index(env, name, &invoice.id); }
            }
            if old.metadata_tax_id != invoice.metadata_tax_id {
                if let Some(ref tax_id) = old.metadata_tax_id { Self::remove_from_tax_id_index(env, tax_id, &invoice.id); }
                if let Some(ref tax_id) = invoice.metadata_tax_id { Self::add_to_tax_id_index(env, tax_id, &invoice.id); }
            }
        }
        env.storage().persistent().set(&DataKey::Invoice(invoice.id.clone()), invoice);
    }

    pub fn get_by_business(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_business(business)).unwrap_or(Vec::new(env))
    }

    pub fn get_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_status(status)).unwrap_or(Vec::new(env))
    }

    /// Get all invoice IDs that have ever been disputed.
    pub fn get_dispute_index(env: &Env) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::dispute_index()).unwrap_or_else(|| Vec::new(env))
    }

    /// Add an invoice to the dispute index (append-only discovery index).
    pub fn add_to_dispute_index(env: &Env, invoice_id: &BytesN<32>) {
        let mut ids = Self::get_dispute_index(env);
        if !ids.iter().any(|id| id == *invoice_id) {
            ids.push_back(invoice_id.clone());
            env.storage().persistent().set(&Indexes::dispute_index(), &ids);
        }
    }

    /// Increment and return the total invoice counter.
    pub fn next_count(env: &Env) -> u64 {
        let current: u64 = env.storage().persistent().get(&StorageKeys::invoice_count()).unwrap_or(0);
        let next = current.saturating_add(1);
        env.storage().persistent().set(&StorageKeys::invoice_count(), &next);
        next
    }

    // --- Private Index Helpers ---

    fn add_to_business_index(env: &Env, business: &Address, id: &BytesN<32>) {
        let mut list = Self::get_by_business(env, business);
        if !list.contains(id) {
            list.push_back(id.clone());
            env.storage().persistent().set(&Indexes::invoices_by_business(business), &list);
        }
    }

    fn add_to_status_index(env: &Env, status: InvoiceStatus, id: &BytesN<32>) {
        let mut list = Self::get_by_status(env, status.clone());
        if !list.contains(id) {
            list.push_back(id.clone());
            env.storage().persistent().set(&Indexes::invoices_by_status(status), &list);
        }
    }

    fn remove_from_status_index(env: &Env, status: InvoiceStatus, id: &BytesN<32>) {
        let mut list = Self::get_by_status(env, status.clone());
        if let Some(pos) = list.iter().position(|x| x == *id) {
            list.remove(pos as u32);
            env.storage().persistent().set(&Indexes::invoices_by_status(status), &list);
        }
    }

    fn add_to_customer_index(env: &Env, name: &String, id: &BytesN<32>) {
        let key = Indexes::invoices_by_customer(name);
        let mut list: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if !list.contains(id) {
            list.push_back(id.clone());
            env.storage().persistent().set(&key, &list);
        }
    }

    fn remove_from_customer_index(env: &Env, name: &String, id: &BytesN<32>) {
        let key = Indexes::invoices_by_customer(name);
        let mut list: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if let Some(pos) = list.iter().position(|x| x == *id) {
            list.remove(pos as u32);
            env.storage().persistent().set(&key, &list);
        }
    }

    fn add_to_tax_id_index(env: &Env, tax_id: &String, id: &BytesN<32>) {
        let key = Indexes::invoices_by_tax_id(tax_id);
        let mut list: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if !list.contains(id) {
            list.push_back(id.clone());
            env.storage().persistent().set(&key, &list);
        }
    }

    fn remove_from_tax_id_index(env: &Env, tax_id: &String, id: &BytesN<32>) {
        let key = Indexes::invoices_by_tax_id(tax_id);
        let mut list: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or(Vec::new(env));
        if let Some(pos) = list.iter().position(|x| x == *id) {
            list.remove(pos as u32);
            env.storage().persistent().set(&key, &list);
        }
    }

    // --- Public API Aliases & Extended Queries ---

    pub fn store_invoice(env: &Env, invoice: &Invoice) { Self::store(env, invoice); }
    pub fn get_invoice(env: &Env, id: &BytesN<32>) -> Option<Invoice> { Self::get(env, id) }
    pub fn update_invoice(env: &Env, invoice: &Invoice) { Self::update(env, invoice); }
    pub fn get_invoices_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> { Self::get_by_status(env, status) }
    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> { Self::get_by_business(env, business) }

    pub fn count_active_business_invoices(env: &Env, business: &Address) -> u32 {
        let mut count = 0u32;
        for id in Self::get_by_business(env, business).iter() {
            if let Some(inv) = Self::get(env, &id) {
                if crate::protocol_limits::is_active_status(&inv.status) { count += 1; }
            }
        }
        count
    }

    pub fn get_invoices_by_customer(env: &Env, name: &String) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_customer(name)).unwrap_or(Vec::new(env))
    }

    pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        env.storage().persistent().get(&Indexes::invoices_by_tax_id(tax_id)).unwrap_or(Vec::new(env))
    }

    pub fn get_invoices_by_category(env: &Env, cat: &crate::invoice::InvoiceCategory) -> Vec<BytesN<32>> {
        let mut res = Vec::new(env);
        for id in Self::get_all_invoice_ids(env).iter() {
            if let Some(inv) = Self::get(env, &id) {
                if inv.category == *cat { res.push_back(id); }
            }
        }
        res
    }

    pub fn get_invoices_by_category_and_status(env: &Env, cat: &crate::invoice::InvoiceCategory, status: &InvoiceStatus) -> Vec<BytesN<32>> {
        let mut res = Vec::new(env);
        for id in Self::get_by_status(env, status.clone()).iter() {
            if let Some(inv) = Self::get(env, &id) {
                if inv.category == *cat { res.push_back(id); }
            }
        }
        res
    }

    pub fn get_invoices_by_tag(env: &Env, tag: &String) -> Vec<BytesN<32>> {
        let mut res = Vec::new(env);
        for id in Self::get_all_invoice_ids(env).iter() {
            if let Some(inv) = Self::get(env, &id) {
                if inv.has_tag(tag.clone()) { res.push_back(id); }
            }
        }
        res
    }

    pub fn get_invoices_by_tags(env: &Env, tags: &Vec<String>) -> Vec<BytesN<32>> {
        let mut res = Vec::new(env);
        'outer: for id in Self::get_all_invoice_ids(env).iter() {
            if let Some(inv) = Self::get(env, &id) {
                for tag in tags.iter() {
                    if !inv.has_tag(tag) { continue 'outer; }
                }
                res.push_back(id);
            }
        }
        res
    }

    pub fn get_all_invoice_ids(env: &Env) -> Vec<BytesN<32>> {
        let mut all = Vec::new(env);
        let statuses = [
            InvoiceStatus::Pending, InvoiceStatus::Verified, InvoiceStatus::Funded,
            InvoiceStatus::Paid, InvoiceStatus::Defaulted, InvoiceStatus::Cancelled,
            InvoiceStatus::Refunded
        ];
        for s in statuses {
            for id in Self::get_by_status(env, s).iter() {
                if !all.contains(&id) { all.push_back(id); }
            }
        }
        all
    }

    pub fn clear_all(env: &Env) { StorageManager::clear_all_mappings(env); }
}

/// Storage operations for bids
pub struct BidStorage;
impl BidStorage {
    pub fn store(env: &Env, bid: &Bid) {
        env.storage().persistent().set(&DataKey::Bid(bid.bid_id.clone()), bid);
        Self::add_to_invoice_index(env, &bid.invoice_id, &bid.bid_id);
        Self::add_to_investor_index(env, &bid.investor, &bid.bid_id);
        Self::add_to_status_index(env, bid.status, &bid.bid_id);
    }
    pub fn get(env: &Env, id: &BytesN<32>) -> Option<Bid> { env.storage().persistent().get(&DataKey::Bid(id.clone())) }
    pub fn update(env: &Env, bid: &Bid) {
        if let Some(old) = Self::get(env, &bid.bid_id) {
            if old.status != bid.status {
                Self::remove_from_status_index(env, old.status, &bid.bid_id);
                Self::add_to_status_index(env, bid.status, &bid.bid_id);
            }
        }
        env.storage().persistent().set(&DataKey::Bid(bid.bid_id.clone()), bid);
    }
    pub fn get_by_invoice(env: &Env, id: &BytesN<32>) -> Vec<BytesN<32>> { env.storage().persistent().get(&Indexes::bids_by_invoice(id)).unwrap_or(Vec::new(env)) }
    pub fn get_by_investor(env: &Env, addr: &Address) -> Vec<BytesN<32>> { env.storage().persistent().get(&Indexes::bids_by_investor(addr)).unwrap_or(Vec::new(env)) }
    pub fn get_by_status(env: &Env, status: BidStatus) -> Vec<BytesN<32>> { env.storage().persistent().get(&Indexes::bids_by_status(status)).unwrap_or(Vec::new(env)) }

    fn add_to_invoice_index(env: &Env, inv: &BytesN<32>, bid: &BytesN<32>) {
        let mut list = Self::get_by_invoice(env, inv);
        if !list.contains(bid) { list.push_back(bid.clone()); env.storage().persistent().set(&Indexes::bids_by_invoice(inv), &list); }
    }
    fn add_to_investor_index(env: &Env, invr: &Address, bid: &BytesN<32>) {
        let mut list = Self::get_by_investor(env, invr);
        if !list.contains(bid) { list.push_back(bid.clone()); env.storage().persistent().set(&Indexes::bids_by_investor(invr), &list); }
    }
    fn add_to_status_index(env: &Env, s: BidStatus, bid: &BytesN<32>) {
        let mut list = Self::get_by_status(env, s);
        if !list.contains(bid) { list.push_back(bid.clone()); env.storage().persistent().set(&Indexes::bids_by_status(s), &list); }
    }
    fn remove_from_status_index(env: &Env, s: BidStatus, bid: &BytesN<32>) {
        let mut list = Self::get_by_status(env, s);
        if let Some(pos) = list.iter().position(|x| x == *bid) { list.remove(pos as u32); env.storage().persistent().set(&Indexes::bids_by_status(s), &list); }
    }
    pub fn next_count(env: &Env) -> u64 {
        let c: u64 = env.storage().persistent().get(&StorageKeys::bid_count()).unwrap_or(0);
        let n = c.saturating_add(1); env.storage().persistent().set(&StorageKeys::bid_count(), &n); n
    }
}

/// Storage operations for investments
pub struct InvestmentStorage;
impl InvestmentStorage {
    pub fn store(env: &Env, i: &Investment) {
        env.storage().persistent().set(&DataKey::Investment(i.investment_id.clone()), i);
        Self::add_to_invoice_index(env, &i.invoice_id, &i.investment_id);
        Self::add_to_investor_index(env, &i.investor, &i.investment_id);
        Self::add_to_status_index(env, i.status, &i.investment_id);
    }
    pub fn get(env: &Env, id: &BytesN<32>) -> Option<Investment> { env.storage().persistent().get(&DataKey::Investment(id.clone())) }
    pub fn update(env: &Env, i: &Investment) {
        if let Some(old) = Self::get(env, &i.investment_id) {
            if old.status != i.status {
                Self::remove_from_status_index(env, old.status, &i.investment_id);
                Self::add_to_status_index(env, i.status, &i.investment_id);
            }
        }
        env.storage().persistent().set(&DataKey::Investment(i.investment_id.clone()), i);
    }
    pub fn get_by_invoice(env: &Env, id: &BytesN<32>) -> Vec<BytesN<32>> { env.storage().persistent().get(&Indexes::investments_by_invoice(id)).unwrap_or(Vec::new(env)) }
    pub fn get_by_investor(env: &Env, addr: &Address) -> Vec<BytesN<32>> { env.storage().persistent().get(&Indexes::investments_by_investor(addr)).unwrap_or(Vec::new(env)) }
    pub fn get_by_status(env: &Env, s: InvestmentStatus) -> Vec<BytesN<32>> { env.storage().persistent().get(&Indexes::investments_by_status(s)).unwrap_or(Vec::new(env)) }

    fn add_to_invoice_index(env: &Env, inv: &BytesN<32>, id: &BytesN<32>) {
        let mut list = Self::get_by_invoice(env, inv);
        if !list.contains(id) { list.push_back(id.clone()); env.storage().persistent().set(&Indexes::investments_by_invoice(inv), &list); }
    }
    fn add_to_investor_index(env: &Env, invr: &Address, id: &BytesN<32>) {
        let mut list = Self::get_by_investor(env, invr);
        if !list.contains(id) { list.push_back(id.clone()); env.storage().persistent().set(&Indexes::investments_by_investor(invr), &list); }
    }
    fn add_to_status_index(env: &Env, s: InvestmentStatus, id: &BytesN<32>) {
        let mut list = Self::get_by_status(env, s);
        if !list.contains(id) { list.push_back(id.clone()); env.storage().persistent().set(&Indexes::investments_by_status(s), &list); }
    }
    fn remove_from_status_index(env: &Env, s: InvestmentStatus, id: &BytesN<32>) {
        let mut list = Self::get_by_status(env, s);
        if let Some(pos) = list.iter().position(|x| x == *id) { list.remove(pos as u32); env.storage().persistent().set(&Indexes::investments_by_status(s), &list); }
    }
    pub fn next_count(env: &Env) -> u64 {
        let c: u64 = env.storage().persistent().get(&StorageKeys::investment_count()).unwrap_or(0);
        let n = c.saturating_add(1); env.storage().persistent().set(&StorageKeys::investment_count(), &n); n
    }
}

pub struct ConfigStorage;
impl ConfigStorage {
    pub fn set_platform_fees(env: &Env, c: &PlatformFeeConfig) { env.storage().instance().set(&StorageKeys::platform_fees(), c); }
    pub fn get_platform_fees(env: &Env) -> Option<PlatformFeeConfig> { env.storage().instance().get(&StorageKeys::platform_fees()) }
}

pub struct StorageManager;
impl StorageManager {
    pub fn clear_all_mappings(env: &Env) {
        env.storage().persistent().remove(&StorageKeys::invoice_count());
        env.storage().persistent().remove(&StorageKeys::bid_count());
        env.storage().persistent().remove(&StorageKeys::investment_count());
        env.storage().persistent().remove(&Indexes::dispute_index());
    }
}
