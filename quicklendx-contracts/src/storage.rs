//! Storage management for the QuickLendX invoice factoring protocol.
//!
//! This module defines storage keys, indexing strategies, and storage operations
//! for efficient data retrieval and management.
//!
//! #[derive(Clone, Debug, Eq, PartialEq)]Storage Design
//!
//! The storage is organized with the following indexing strategy:
//! - Primary storage: Direct key-value storage for core entities
//! - Secondary indexes: For efficient querying by various criteria
//!
//! #[derive(Clone, Debug, Eq, PartialEq)]Security Notes
//!
//! - Storage keys use symbols to prevent collisions
//! - Instance storage is used for frequently accessed data
//! - Persistent storage for long-term data retention
//! - Upgrade-safe: Keys are designed to avoid conflicts during contract upgrades

use crate::bid::BidStatus;
use crate::investment::InvestmentStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::profits::PlatformFeeConfig;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, String, Symbol};

/// Data keys for namespacing contract storage
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    // Core Entities
    Invoice(BytesN<32>),
    Bid(BytesN<32>),
    Investment(BytesN<32>),
    Escrow(BytesN<32>),

    // Counters
    InvoiceCount,
    BidCount,
    InvestmentCount,

    // Config
    Admin,
    PlatformFee,
    PlatformFeeConfig,
    Treasury,
    BidTTL,
    MaxActiveBidsPerInvestor,

    // Secondary Indexes (Status)
    InvoicesByStatus(Symbol),
    BidsByStatus(Symbol),
    InvestmentsByStatus(Symbol),

    // Secondary Indexes (Foreign Keys)
    InvoicesByBusiness(Address),
    InvoicesByCustomer(String),
    InvoicesByTaxId(String),
    InvoicesByCategory(Symbol),
    InvoicesByTag(String),
    BidsByInvoice(BytesN<32>),
    BidsByInvestor(Address),
    InvestmentsByInvoice(BytesN<32>),
    InvestmentsByInvestor(Address),

    // Verification
    BusinessVerification(Address),
    InvestorVerification(Address),
    VerifiedBusinesses,
    PendingBusinesses,
    RejectedBusinesses,
    VerifiedInvestors,
    PendingInvestors,
    RejectedInvestors,

    // Other
    VendorInvoiceCount(Address),
    EscrowByInvoice(BytesN<32>),
    PlatformTreasury,
}

/// Storage keys for the contract
pub struct StorageKeys;

impl StorageKeys {
    /// Key for storing invoices by ID
    pub fn invoice(invoice_id: &BytesN<32>) -> DataKey {
        DataKey::Invoice(invoice_id.clone())
    }

    /// Key for storing bids by ID
    pub fn bid(bid_id: &BytesN<32>) -> DataKey {
        DataKey::Bid(bid_id.clone())
    }

    /// Key for storing investments by ID
    pub fn investment(investment_id: &BytesN<32>) -> DataKey {
        DataKey::Investment(investment_id.clone())
    }

    /// Key for platform fee configuration
    pub fn platform_fees() -> DataKey {
        DataKey::PlatformFeeConfig
    }

    /// Key for invoice count
    pub fn invoice_count() -> DataKey {
        DataKey::InvoiceCount
    }

    /// Key for bid count
    pub fn bid_count() -> DataKey {
        DataKey::BidCount
    }

    /// Key for investment count
    pub fn investment_count() -> DataKey {
        DataKey::InvestmentCount
    }
    /// Key for storing an escrow record by its ID
    pub fn escrow(escrow_id: &BytesN<32>) -> DataKey {
        DataKey::Escrow(escrow_id.clone())
    }

    /// Key for mapping an invoice ID to its escrow record ID
    pub fn escrow_inv_key(invoice_id: &BytesN<32>) -> DataKey {
        DataKey::EscrowByInvoice(invoice_id.clone())
    }
}

/// Secondary indexes for efficient querying
pub struct Indexes;

impl Indexes {
    /// Index: invoices by business address
    pub fn invoices_by_business(business: &Address) -> DataKey {
        DataKey::InvoicesByBusiness(business.clone())
    }

    /// Index: invoices by status
    pub fn invoices_by_status(status: InvoiceStatus) -> DataKey {
        let status_symbol = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("defaulted"),
            InvoiceStatus::Cancelled => symbol_short!("cancelled"),
            InvoiceStatus::Refunded => symbol_short!("refunded"),
        };
        DataKey::InvoicesByStatus(status_symbol)
    }

    /// Index: bids by invoice
    pub fn bids_by_invoice(invoice_id: &BytesN<32>) -> DataKey {
        DataKey::BidsByInvoice(invoice_id.clone())
    }

    /// Index: bids by investor
    pub fn bids_by_investor(investor: &Address) -> DataKey {
        DataKey::BidsByInvestor(investor.clone())
    }

    /// Index: bids by status
    pub fn bids_by_status(status: BidStatus) -> DataKey {
        let status_symbol = match status {
            BidStatus::Placed => symbol_short!("placed"),
            BidStatus::Withdrawn => symbol_short!("withdrawn"),
            BidStatus::Accepted => symbol_short!("accepted"),
            BidStatus::Expired => symbol_short!("expired"),
            BidStatus::Cancelled => symbol_short!("cancelled"),
        };
        DataKey::BidsByStatus(status_symbol)
    }

    /// Index: investments by invoice
    pub fn investments_by_invoice(invoice_id: &BytesN<32>) -> DataKey {
        DataKey::InvestmentsByInvoice(invoice_id.clone())
    }

    /// Index: investments by investor
    pub fn investments_by_investor(investor: &Address) -> DataKey {
        DataKey::InvestmentsByInvestor(investor.clone())
    }

    /// Index: invoices by customer name
    pub fn invoices_by_customer(customer_name: &String) -> DataKey {
        DataKey::InvoicesByCustomer(customer_name.clone())
    }

    /// Index: invoices by tax_id
    pub fn invoices_by_tax_id(tax_id: &String) -> DataKey {
        DataKey::InvoicesByTaxId(tax_id.clone())
    }

    /// Index: invoices by category
    pub fn invoices_by_category(category: InvoiceCategory) -> DataKey {
        let category_symbol = match category {
            InvoiceCategory::Services => symbol_short!("services"),
            InvoiceCategory::Products => symbol_short!("products"),
            InvoiceCategory::Consulting => symbol_short!("consult"),
            InvoiceCategory::Manufacturing => symbol_short!("manufact"),
            InvoiceCategory::Technology => symbol_short!("tech"),
            InvoiceCategory::Healthcare => symbol_short!("health"),
            InvoiceCategory::Other => symbol_short!("other"),
        };
        DataKey::InvoicesByCategory(category_symbol)
    }

    /// Index: investments by status
    pub fn investments_by_status(status: InvestmentStatus) -> DataKey {
        let status_symbol = match status {
            InvestmentStatus::Active => symbol_short!("active"),
            InvestmentStatus::Completed => symbol_short!("complete"),
            InvestmentStatus::Defaulted => symbol_short!("default"),
            InvestmentStatus::Refunded => symbol_short!("refund"),
            InvestmentStatus::Withdrawn => symbol_short!("withdraw"),
        };
        DataKey::InvestmentsByStatus(status_symbol)
    }

    /// Index: invoices by tag
    pub fn invoices_by_tag(tag: &String) -> DataKey {
        DataKey::InvoicesByTag(tag.clone())
    }
}

/// Storage operations for invoices
pub struct InvoiceStorage;

impl InvoiceStorage {
    /// Store an invoice
    pub fn store_invoice(env: &Env, invoice: &Invoice) {
        env.storage()
            .persistent()
            .set(&StorageKeys::invoice(&invoice.id), invoice);

        // Add to business invoices list
        Self::add_to_business_invoices(env, &invoice.business, &invoice.id);

        // Add to status invoices list
        Self::add_to_status_invoices(env, invoice.status, &invoice.id);

        // Add to category index
        Self::add_category_index(env, invoice.category.clone(), &invoice.id);

        // Add to tag indexes
        for tag in invoice.tags.iter() {
            Self::add_tag_index(env, &tag, &invoice.id);
        }

        // Add metadata indexes
        Self::add_metadata_indexes(env, invoice);
    }

    /// Get an invoice by ID
    pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        env.storage()
            .persistent()
            .get(&StorageKeys::invoice(invoice_id))
    }

    /// Update an invoice
    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        if let Some(old_invoice) = Self::get_invoice(env, &invoice.id) {
            if old_invoice.status != invoice.status {
                // Remove from old status list
                Self::remove_from_status_invoices(env, old_invoice.status, &invoice.id);
                // Add to new status list
                Self::add_to_status_invoices(env, invoice.status, &invoice.id);
            }
            if old_invoice.category.clone() != invoice.category {
                Self::remove_category_index(env, old_invoice.category.clone(), &invoice.id);
                Self::add_category_index(env, invoice.category.clone(), &invoice.id);
            }
            // Update metadata indexes if they changed
            if old_invoice.metadata_customer_name != invoice.metadata_customer_name
                || old_invoice.metadata_tax_id != invoice.metadata_tax_id
            {
                if let Some(md) = old_invoice.metadata() {
                    Self::remove_metadata_indexes(env, &md, &invoice.id);
                }
                Self::add_metadata_indexes(env, invoice);
            }
        }
        env.storage()
            .persistent()
            .set(&StorageKeys::invoice(&invoice.id), invoice);
    }

    /// Get all invoices for a business
    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_business(business))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get all invoices by status
    pub fn get_invoices_by_status(env: &Env, status: InvoiceStatus) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_status(status))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get invoices by customer name
    pub fn get_invoices_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_customer(customer_name))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get invoices by tax ID
    pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_tax_id(tax_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get invoices by category
    pub fn get_invoices_by_category(env: &Env, category: InvoiceCategory) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_category(category))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get invoices by category and status
    pub fn get_invoices_by_category_and_status(
        env: &Env,
        category: InvoiceCategory,
        status: InvoiceStatus,
    ) -> Vec<BytesN<32>> {
        let cat_invoices = Self::get_invoices_by_category(env, category);
        let mut filtered = Vec::new(env);
        for id in cat_invoices.iter() {
            if let Some(invoice) = Self::get_invoice(env, &id) {
                if invoice.status == status {
                    filtered.push_back(id);
                }
            }
        }
        filtered
    }

    fn add_to_business_invoices(env: &Env, business: &Address, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_business(business);
        let mut invoices = Self::get_business_invoices(env, business);
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &invoices);
        }
    }

    pub fn add_to_status_invoices(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_status(status);
        let mut invoices = Self::get_invoices_by_status(env, status);
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &invoices);
        }
    }

    pub fn remove_from_status_invoices(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_status(status);
        let mut invoices = Self::get_invoices_by_status(env, status);
        if let Some(pos) = invoices.iter().position(|a| a == *invoice_id) {
            invoices.remove(pos as u32);
            env.storage().persistent().set(&key, &invoices);
        }
    }

    pub fn add_category_index(env: &Env, category: InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_category(category);
        let mut invoices: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &invoices);
        }
    }

    pub fn remove_category_index(env: &Env, category: InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_category(category);
        if let Some(mut invoices) = env.storage().persistent().get::<_, Vec<BytesN<32>>>(&key) {
            if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
                invoices.remove(pos as u32);
                env.storage().persistent().set(&key, &invoices);
            }
        }
    }

    pub fn add_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tag(tag);
        let mut invoices: Vec<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage().persistent().set(&key, &invoices);
        }
    }

    pub fn remove_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Indexes::invoices_by_tag(tag);
        if let Some(mut invoices) = env.storage().persistent().get::<_, Vec<BytesN<32>>>(&key) {
            if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
                invoices.remove(pos as u32);
                env.storage().persistent().set(&key, &invoices);
            }
        }
    }

    pub fn add_metadata_indexes(env: &Env, invoice: &Invoice) {
        if let Some(ref name) = invoice.metadata_customer_name {
            let key = Indexes::invoices_by_customer(name);
            let mut ids: Vec<BytesN<32>> = env
                .storage()
                .persistent()
                .get(&key)
                .unwrap_or(Vec::new(env));
            if !ids.contains(&invoice.id) {
                ids.push_back(invoice.id.clone());
                env.storage().persistent().set(&key, &ids);
            }
        }
        if let Some(ref tax_id) = invoice.metadata_tax_id {
            let key = Indexes::invoices_by_tax_id(tax_id);
            let mut ids: Vec<BytesN<32>> = env
                .storage()
                .persistent()
                .get(&key)
                .unwrap_or(Vec::new(env));
            if !ids.contains(&invoice.id) {
                ids.push_back(invoice.id.clone());
                env.storage().persistent().set(&key, &ids);
            }
        }
    }

    pub fn remove_metadata_indexes(env: &Env, metadata: &InvoiceMetadata, invoice_id: &BytesN<32>) {
        // This is tricky because metadata in Invoice struct is fragmented but helper returns a struct.
        // We'll use the fields directly from Invoice in update_invoice, or use this helper.
        let key_c = Indexes::invoices_by_customer(&metadata.customer_name);
        if let Some(mut ids) = env.storage().persistent().get::<_, Vec<BytesN<32>>>(&key_c) {
            if let Some(pos) = ids.iter().position(|id| id == *invoice_id) {
                ids.remove(pos as u32);
                env.storage().persistent().set(&key_c, &ids);
            }
        }
        let key_t = Indexes::invoices_by_tax_id(&metadata.tax_id);
        if let Some(mut ids) = env.storage().persistent().get::<_, Vec<BytesN<32>>>(&key_t) {
            if let Some(pos) = ids.iter().position(|id| id == *invoice_id) {
                ids.remove(pos as u32);
                env.storage().persistent().set(&key_t, &ids);
            }
        }
    }

    /// Clear all invoice data (admin only, emergency)
    /// Clear all invoice data (admin only, emergency)
    pub fn clear_all(env: &Env) {
        let all_statuses = [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
            InvoiceStatus::Cancelled,
            InvoiceStatus::Refunded,
        ];

        for status in all_statuses {
            let ids = Self::get_invoices_by_status(env, status);
            for id in ids.iter() {
                env.storage().persistent().remove(&StorageKeys::invoice(&id));
            }
            env.storage()
                .persistent()
                .remove(&Indexes::invoices_by_status(status));
        }
    }

    pub fn clear_all_invoices(env: &Env) {
        Self::clear_all(env);
    }

    /// Count active invoices for a business (excludes Cancelled, Paid, and Refunded invoices)
    pub fn count_active_business_invoices(env: &Env, business: &Address) -> u32 {
        let business_invoices = Self::get_business_invoices(env, business);
        let mut count = 0u32;
        for invoice_id in business_invoices.iter() {
            if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                if !matches!(
                    invoice.status,
                    InvoiceStatus::Cancelled | InvoiceStatus::Paid | InvoiceStatus::Refunded
                ) {
                    count = count.saturating_add(1);
                }
            }
        }
        count
    }

    /// Get rating statistics for a specific invoice from storage
    pub fn get_invoice_rating_stats(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Option<InvoiceRatingStats> {
        Self::get_invoice(env, invoice_id).map(|inv| inv.get_invoice_rating_stats())
    }

    /// Completely remove an invoice from storage and all its indexes
    pub fn remove_invoice(env: &Env, invoice_id: &BytesN<32>) {
        if let Some(invoice) = Self::get_invoice(env, invoice_id) {
            // Remove from status list
            Self::remove_from_status_invoices(env, invoice.status, invoice_id);

            // Remove from business index
            let business_key = Indexes::invoices_by_business(&invoice.business);
            if let Some(mut invoices) = env
                .storage()
                .persistent()
                .get::<_, Vec<BytesN<32>>>(&business_key)
            {
                if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
                    invoices.remove(pos as u32);
                    env.storage().persistent().set(&business_key, &invoices);
                }
            }

            Self::remove_category_index(env, invoice.category.clone(), invoice_id);

            for tag in invoice.tags.iter() {
                Self::remove_tag_index(env, &tag, invoice_id);
            }

            if let Some(md) = invoice.metadata() {
                Self::remove_metadata_indexes(env, &md, invoice_id);
            }
            env.storage()
                .persistent()
                .remove(&StorageKeys::invoice(invoice_id));
        }
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

    /// Get count of invoices with ratings
    pub fn get_invoices_with_ratings_count(env: &Env) -> u32 {
        let mut count = 0u32;
        let all_statuses = [
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Cancelled,
            InvoiceStatus::Refunded,
        ];
        let mut all_invoices: Vec<Invoice> = Vec::new(env);
        for status in all_statuses {
            let invoices = Self::get_invoices_by_status(env, status);
            for id in invoices.iter() {
                if let Some(inv) = Self::get_invoice(env, &id) {
                    all_invoices.push_back(inv);
                }
            }
        }

        for invoice in all_invoices.iter() {
            if invoice.average_rating.is_some() {
                count += 1;
            }
        }
        count
    }
    /// Get all available categories
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
    /// Get invoices by tag
    pub fn get_invoices_by_tag(env: &Env, tag: &String) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::invoices_by_tag(tag))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get invoices by multiple tags (AND logic)
    pub fn get_invoices_by_tags(env: &Env, tags: &Vec<String>) -> Vec<BytesN<32>> {
        if tags.is_empty() {
            return Vec::new(env);
        }
        let mut result = Vec::new(env);
        let first_tag = tags.get(0).unwrap();
        let initial_ids = Self::get_invoices_by_tag(env, &first_tag);
        
        for id in initial_ids.iter() {
            let mut matches_all = true;
            if let Some(invoice) = Self::get_invoice(env, &id) {
                for i in 1..tags.len() {
                    let tag = tags.get(i).unwrap();
                    if !invoice.has_tag(tag) {
                        matches_all = false;
                        break;
                    }
                }
            } else {
                matches_all = false;
            }
            if matches_all {
                result.push_back(id);
            }
        }
        result
    }

    /// Get invoice count by category
    pub fn get_invoice_count_by_category(env: &Env, category: InvoiceCategory) -> u32 {
        Self::get_invoices_by_category(env, category).len()
    }

    /// Get invoice count by tag
    pub fn get_invoice_count_by_tag(env: &Env, tag: &String) -> u32 {
        Self::get_invoices_by_tag(env, tag).len()
    }
}

/// Storage operations for bids
pub struct BidStorage;

impl BidStorage {
    /// Store a bid
    pub fn store_bid(env: &Env, bid: &Bid) {
        env.storage()
            .persistent()
            .set(&StorageKeys::bid(&bid.bid_id), bid);

        // Update indexes
        Self::add_to_invoice_index(env, &bid.invoice_id, &bid.bid_id);
        Self::add_to_investor_index(env, &bid.investor, &bid.bid_id);
        Self::add_to_status_index(env, bid.status.clone(), &bid.bid_id);
    }

    /// Get a bid by ID
    pub fn get_bid(env: &Env, bid_id: &BytesN<32>) -> Option<Bid> {
        env.storage().persistent().get(&StorageKeys::bid(bid_id))
    }

    /// Update a bid
    pub fn update_bid(env: &Env, bid: &Bid) {
        env.storage()
            .persistent()
            .set(&StorageKeys::bid(&bid.bid_id), bid);
        Self::add_to_status_index(env, bid.status, &bid.bid_id);
    }

    /// Add a bid to an invoice's index
    pub fn add_bid_to_invoice(env: &Env, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        Self::add_to_invoice_index(env, invoice_id, bid_id);
    }

    pub fn next_count(env: &Env) -> u64 {
        let counter_key = StorageKeys::bid_count();
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0);
        let next_counter = counter.saturating_add(1);
        env.storage().instance().set(&counter_key, &next_counter);
        next_counter
    }

    /// Get bids by invoice
    pub fn get_bids_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::bids_by_invoice(invoice_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get bids by investor
    pub fn get_bids_by_investor_all(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::bids_by_investor(investor))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get bids by status
    pub fn get_bids_by_status_key(env: &Env, status: BidStatus) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::bids_by_status(status))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Add bid to invoice index
    fn add_to_invoice_index(env: &Env, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        let key = Indexes::bids_by_invoice(invoice_id);
        let mut bids = Self::get_bids_for_invoice(env, invoice_id);
        if !bids.contains(bid_id) {
            bids.push_back(bid_id.clone());
            env.storage().persistent().set(&key, &bids);
        }
    }

    /// Add bid to investor index
    fn add_to_investor_index(env: &Env, investor: &Address, bid_id: &BytesN<32>) {
        let key = Indexes::bids_by_investor(investor);
        let mut bids = Self::get_bids_by_investor_all(env, investor);
        if !bids.contains(bid_id) {
            bids.push_back(bid_id.clone());
            env.storage().persistent().set(&key, &bids);
        }
    }

    /// Add bid to status index
    fn add_to_status_index(env: &Env, status: BidStatus, bid_id: &BytesN<32>) {
        let key = Indexes::bids_by_status(status.clone());
        let mut bids = Self::get_bids_by_status_key(env, status);
        if !bids.contains(bid_id) {
            bids.push_back(bid_id.clone());
            env.storage().persistent().set(&key, &bids);
        }
    }

    /// Remove bid from status index
    fn remove_from_status_index(env: &Env, status: BidStatus, bid_id: &BytesN<32>) {
        let key = Indexes::bids_by_status(status.clone());
        let mut bids = Self::get_bids_by_status_key(env, status);
        if let Some(pos) = bids.iter().position(|id| id == *bid_id) {
            bids.remove(pos as u32);
            env.storage().persistent().set(&key, &bids);
        }
    }

    pub fn get_bid_ttl_days(env: &Env) -> u64 {
        env.storage().instance().get(&DataKey::BidTTL).unwrap_or(7) // DEFAULT_BID_TTL_DAYS
    }

    pub fn set_bid_ttl_days(env: &Env, days: u64) {
        env.storage().instance().set(&DataKey::BidTTL, &days);
    }

    pub fn get_max_active_bids_per_investor(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxActiveBidsPerInvestor)
            .unwrap_or(20) // DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR
    }

    pub fn set_max_active_bids_per_investor(env: &Env, limit: u32) {
        env.storage()
            .instance()
            .set(&DataKey::MaxActiveBidsPerInvestor, &limit);
    }

    pub fn cleanup_expired_bids(env: &Env, invoice_id: &BytesN<32>) -> u32 {
        let current_timestamp = env.ledger().timestamp();
        let bid_ids = Self::get_bids_for_invoice(env, invoice_id);
        let mut active = Vec::new(env);
        let mut expired_count = 0u32;
        let ttl_seconds = Self::get_bid_ttl_days(env) * 86400;

        for bid_id in bid_ids.iter() {
            if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                if bid.status == BidStatus::Placed
                    && bid.timestamp.saturating_add(ttl_seconds) < current_timestamp
                {
                    bid.status = BidStatus::Expired;
                    Self::update_bid(env, &bid);
                    expired_count += 1;
                } else {
                    active.push_back(bid_id);
                }
            }
        }

        if expired_count > 0 {
            env.storage()
                .persistent()
                .set(&Indexes::bids_by_invoice(invoice_id), &active);
        }

        expired_count
    }

    pub fn get_bid_records_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<Bid> {
        Self::cleanup_expired_bids(env, invoice_id);
        let mut bids = Vec::new(env);
        for bid_id in Self::get_bids_for_invoice(env, invoice_id).iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                bids.push_back(bid);
            }
        }
        bids
    }

    pub fn get_bids_by_status(env: &Env, invoice_id: &BytesN<32>, status: BidStatus) -> Vec<Bid> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut filtered = Vec::new(env);
        for bid in records.iter() {
            if bid.status == status {
                filtered.push_back(bid);
            }
        }
        filtered
    }

    pub fn get_bids_by_investor(
        env: &Env,
        invoice_id: &BytesN<32>,
        investor: &Address,
    ) -> Vec<Bid> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut filtered = Vec::new(env);
        for bid in records.iter() {
            if bid.investor == *investor {
                filtered.push_back(bid);
            }
        }
        filtered
    }

    pub fn compare_bids(bid1: &Bid, bid2: &Bid) -> core::cmp::Ordering {
        let profit1 = bid1.expected_return.saturating_sub(bid1.bid_amount);
        let profit2 = bid2.expected_return.saturating_sub(bid2.bid_amount);
        if profit1 != profit2 {
            return profit1.cmp(&profit2);
        }
        if bid1.expected_return != bid2.expected_return {
            return bid1.expected_return.cmp(&bid2.expected_return);
        }
        if bid1.bid_amount != bid2.bid_amount {
            return bid1.bid_amount.cmp(&bid2.bid_amount);
        }
        bid2.timestamp.cmp(&bid1.timestamp)
    }

    pub fn get_best_bid(env: &Env, invoice_id: &BytesN<32>) -> Option<Bid> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut best: Option<Bid> = None;
        for candidate in records.iter() {
            if candidate.status != BidStatus::Placed {
                continue;
            }
            best = match best {
                None => Some(candidate),
                Some(current) => {
                    if Self::compare_bids(&candidate, &current) == core::cmp::Ordering::Greater {
                        Some(candidate)
                    } else {
                        Some(current)
                    }
                }
            };
        }
        best
    }

    pub fn rank_bids(env: &Env, invoice_id: &BytesN<32>) -> Vec<Bid> {
        let records = Self::get_bid_records_for_invoice(env, invoice_id);
        let mut remaining = Vec::new(env);
        for bid in records.iter() {
            if bid.status == BidStatus::Placed {
                remaining.push_back(bid);
            }
        }

        let mut ranked = Vec::new(env);
        while remaining.len() > 0 {
            let mut best_idx: Option<u32> = None;
            let mut best_bid: Option<Bid> = None;

            for (i, candidate) in remaining.iter().enumerate() {
                match best_bid {
                    None => {
                        best_idx = Some(i as u32);
                        best_bid = Some(candidate);
                    }
                    Some(ref current) => {
                        if Self::compare_bids(&candidate, &current) == core::cmp::Ordering::Greater
                        {
                            best_idx = Some(i as u32);
                            best_bid = Some(candidate);
                        }
                    }
                }
            }

            if let Some(bid) = best_bid {
                ranked.push_back(bid);
                remaining.remove(best_idx.unwrap());
            } else {
                break;
            }
        }
        ranked
    }

    pub fn cancel_bid(env: &Env, bid_id: &BytesN<32>) -> bool {
        if let Some(mut bid) = Self::get_bid(env, bid_id) {
            if bid.status == BidStatus::Placed {
                bid.status = BidStatus::Cancelled;
                Self::update_bid(env, &bid);
                return true;
            }
        }
        false
    }

    pub fn get_all_bids_by_investor(env: &Env, investor: &Address) -> Vec<Bid> {
        let bid_ids = Self::get_bids_by_investor_all(env, investor);
        let mut result = Vec::new(env);
        for bid_id in bid_ids.iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                result.push_back(bid);
            }
        }
        result
    }
    pub fn generate_unique_bid_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let next_counter = Self::next_count(env);

        let mut bytes = [0u8; 32];
        bytes[0] = 0xB1; // 'B' for Bid
        bytes[1] = 0xD0; // 'D' for biD
        bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0xB1D0);
        for i in 18..32 {
            bytes[i] = (mix % 256) as u8;
        }
        BytesN::from_array(env, &bytes)
    }

    /// Count active (Placed) bids for an investor across all invoices.
    pub fn count_active_placed_bids_for_investor(env: &Env, investor: &Address) -> u32 {
        let current_timestamp = env.ledger().timestamp();
        let bid_ids = Self::get_bids_by_investor_all(env, investor);
        let mut count = 0u32;

        for bid_id in bid_ids.iter() {
            if let Some(mut bid) = Self::get_bid(env, &bid_id) {
                if bid.status != BidStatus::Placed {
                    continue;
                }
                if bid
                    .timestamp
                    .saturating_add(Self::get_bid_ttl_days(env) * 86400)
                    < current_timestamp
                {
                    bid.status = BidStatus::Expired;
                    Self::update_bid(env, &bid);
                } else {
                    count = count.saturating_add(1);
                }
            }
        }
        count
    }

    pub fn get_active_bid_count(env: &Env, invoice_id: &BytesN<32>) -> u32 {
        let bids = Self::get_bids_for_invoice(env, invoice_id);
        let mut count = 0u32;
        let current_timestamp = env.ledger().timestamp();
        let ttl_seconds = Self::get_bid_ttl_days(env) * 86400;

        for bid_id in bids.iter() {
            if let Some(bid) = Self::get_bid(env, &bid_id) {
                if bid.status == BidStatus::Placed {
                    if bid.timestamp.saturating_add(ttl_seconds) >= current_timestamp {
                        count += 1;
                    }
                }
            }
        }
        count
    }
}

/// Storage operations for investments
pub struct InvestmentStorage;

impl InvestmentStorage {
    /// Store an investment
    pub fn store_investment(env: &Env, investment: &Investment) {
        env.storage().persistent().set(
            &StorageKeys::investment(&investment.investment_id),
            investment,
        );

        // Update indexes
        Self::add_to_invoice_index(env, &investment.invoice_id, &investment.investment_id);
        Self::add_to_investor_index(env, &investment.investor, &investment.investment_id);
        Self::add_to_status_index(env, investment.status.clone(), &investment.investment_id);
    }

    /// Get an investment by ID
    pub fn get_investment(env: &Env, investment_id: &BytesN<32>) -> Option<Investment> {
        env.storage()
            .persistent()
            .get(&StorageKeys::investment(investment_id))
    }

    /// Update an investment
    pub fn update_investment(env: &Env, investment: &Investment) {
        if let Some(old_investment) = Self::get_investment(env, &investment.investment_id) {
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
            &StorageKeys::investment(&investment.investment_id),
            investment,
        );
    }

    pub fn get_investments_for_invoice(env: &Env, invoice_id: &BytesN<32>) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::investments_by_invoice(invoice_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_investments_by_investor(env: &Env, investor: &Address) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::investments_by_investor(investor))
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_investments_by_status(env: &Env, status: InvestmentStatus) -> Vec<BytesN<32>> {
        env.storage()
            .persistent()
            .get(&Indexes::investments_by_status(status))
            .unwrap_or_else(|| Vec::new(env))
    }

    fn add_to_invoice_index(env: &Env, invoice_id: &BytesN<32>, investment_id: &BytesN<32>) {
        let key = Indexes::investments_by_invoice(invoice_id);
        let mut investments = Self::get_investments_for_invoice(env, invoice_id);
        if !investments.contains(investment_id) {
            investments.push_back(investment_id.clone());
            env.storage().persistent().set(&key, &investments);
        }
    }

    fn add_to_investor_index(env: &Env, investor: &Address, investment_id: &BytesN<32>) {
        let key = Indexes::investments_by_investor(investor);
        let mut investments = Self::get_investments_by_investor(env, investor);
        if !investments.contains(investment_id) {
            investments.push_back(investment_id.clone());
            env.storage().persistent().set(&key, &investments);
        }
    }

    fn add_to_status_index(env: &Env, status: InvestmentStatus, investment_id: &BytesN<32>) {
        let key = Indexes::investments_by_status(status.clone());
        let mut investments = Self::get_investments_by_status(env, status);
        if !investments.contains(investment_id) {
            investments.push_back(investment_id.clone());
            env.storage().persistent().set(&key, &investments);
        }
    }

    fn remove_from_status_index(env: &Env, status: InvestmentStatus, investment_id: &BytesN<32>) {
        let key = Indexes::investments_by_status(status.clone());
        let mut investments = Self::get_investments_by_status(env, status);
        if let Some(pos) = investments.iter().position(|id| id == *investment_id) {
            investments.remove(pos as u32);
            env.storage().persistent().set(&key, &investments);
        }
    }

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

    pub fn generate_unique_investment_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let next_counter = Self::next_count(env);

        let mut id_bytes = [0u8; 32];
        id_bytes[0] = 0x1A; // 'I' for Investment
        id_bytes[1] = 0x4E; // 'N' for iNvestment
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0x1A4E);
        for i in 18..32 {
            id_bytes[i] = (mix % 256) as u8;
        }
        BytesN::from_array(env, &id_bytes)
    }

    pub fn get_investment_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Investment> {
        let ids = Self::get_investments_for_invoice(env, invoice_id);
        if let Some(id) = ids.get(0) {
            Self::get_investment(env, &id)
        } else {
            None
        }
    }
}

/// Storage operations for escrows
pub struct EscrowStorage;

impl EscrowStorage {
    fn escrow_key(id: &BytesN<32>) -> DataKey {
        DataKey::Escrow(id.clone())
    }
    /// Map vendor ID -> count of invoices for business
    pub fn vendor_invoice_count(vendor_id: &Address) -> DataKey {
        DataKey::VendorInvoiceCount(vendor_id.clone())
    }

    pub fn escrow_inv_key(invoice_id: &BytesN<32>) -> DataKey {
        DataKey::EscrowByInvoice(invoice_id.clone())
    }

    pub fn store_escrow(env: &Env, escrow: &crate::payments::Escrow) {
        env.storage()
            .persistent()
            .set(&StorageKeys::escrow(&escrow.escrow_id), escrow);
        // Map invoice -> escrow_id
        env.storage().persistent().set(
            &StorageKeys::escrow_inv_key(&escrow.invoice_id),
            &escrow.escrow_id,
        );
    }

    pub fn get_escrow(env: &Env, id: &BytesN<32>) -> Option<crate::payments::Escrow> {
        env.storage()
            .persistent()
            .get(&StorageKeys::escrow(id))
    }

    pub fn get_escrow_by_invoice(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Option<crate::payments::Escrow> {
        let id: Option<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&StorageKeys::escrow_inv_key(invoice_id));
        id.and_then(|id| Self::get_escrow(env, &id))
    }

    /// Backwards compatibility alias
    pub fn escrow_by_invoice_key(env: &Env, invoice_id: &BytesN<32>) -> Option<crate::payments::Escrow> {
        Self::get_escrow_by_invoice(env, invoice_id)
    }

    pub fn update_escrow(env: &Env, escrow: &crate::payments::Escrow) {
        env.storage()
            .persistent()
            .set(&StorageKeys::escrow(&escrow.escrow_id), escrow);
    }

    pub fn generate_unique_escrow_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("esc_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        let next_counter = counter.saturating_add(1);
        env.storage().instance().set(&counter_key, &next_counter);

        let mut id_bytes = [0u8; 32];
        id_bytes[0] = 0xE5;
        id_bytes[1] = 0xC0;
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[10..18].copy_from_slice(&next_counter.to_be_bytes());
        let mix = timestamp
            .saturating_add(next_counter)
            .saturating_add(0xE5C0);
        for i in 18..32 {
            id_bytes[i] = (mix % 256) as u8;
        }
        BytesN::from_array(env, &id_bytes)
    }
}

/// Storage operations for verification
pub struct BusinessVerificationStorage;
impl BusinessVerificationStorage {
    const VERIFIED_BUSINESSES_KEY: &'static str = "verified_businesses";
    const PENDING_BUSINESSES_KEY: &'static str = "pending_businesses";
    const REJECTED_BUSINESSES_KEY: &'static str = "rejected_businesses";

    pub fn store_verification(env: &Env, v: &crate::verification::BusinessVerification) {
        env.storage()
            .instance()
            .set(&DataKey::BusinessVerification(v.business.clone()), v);

        // Update status lists
        match v.status {
            crate::verification::BusinessVerificationStatus::Verified => {
                Self::add_to_verified_businesses(env, &v.business);
            }
            crate::verification::BusinessVerificationStatus::Pending => {
                Self::add_to_pending_businesses(env, &v.business);
            }
            crate::verification::BusinessVerificationStatus::Rejected => {
                Self::add_to_rejected_businesses(env, &v.business);
            }
        }
    }

    pub fn get_verification(
        env: &Env,
        business: &Address,
    ) -> Option<crate::verification::BusinessVerification> {
        Self::get(env, business)
    }

    pub fn get(env: &Env, business: &Address) -> Option<crate::verification::BusinessVerification> {
        env.storage()
            .instance()
            .get(&DataKey::BusinessVerification(business.clone()))
    }

    pub fn update_verification(
        env: &Env,
        verification: &crate::verification::BusinessVerification,
    ) {
        let old_verification = Self::get_verification(env, &verification.business);
        if let Some(old) = old_verification {
            match old.status {
                crate::verification::BusinessVerificationStatus::Verified => {
                    Self::remove_from_verified_businesses(env, &verification.business);
                }
                crate::verification::BusinessVerificationStatus::Pending => {
                    Self::remove_from_pending_businesses(env, &verification.business);
                }
                crate::verification::BusinessVerificationStatus::Rejected => {
                    Self::remove_from_rejected_businesses(env, &verification.business);
                }
            }
        }
        Self::store_verification(env, verification);
    }

    pub fn is_business_verified(env: &Env, business: &Address) -> bool {
        Self::get_verification(env, business)
            .map(|v| v.status == crate::verification::BusinessVerificationStatus::Verified)
            .unwrap_or(false)
    }

    pub fn get_verified_businesses(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::VerifiedBusinesses)
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_pending_businesses(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::PendingBusinesses)
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_rejected_businesses(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::RejectedBusinesses)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn add_to_verified_businesses(env: &Env, business: &Address) {
        let mut verified = Self::get_verified_businesses(env);
        if !verified.contains(business) {
            verified.push_back(business.clone());
            env.storage()
                .instance()
                .set(&DataKey::VerifiedBusinesses, &verified);
        }
    }

    fn add_to_pending_businesses(env: &Env, business: &Address) {
        let mut pending = Self::get_pending_businesses(env);
        if !pending.contains(business) {
            pending.push_back(business.clone());
            env.storage()
                .instance()
                .set(&DataKey::PendingBusinesses, &pending);
        }
    }

    fn add_to_rejected_businesses(env: &Env, business: &Address) {
        let mut rejected = Self::get_rejected_businesses(env);
        if !rejected.contains(business) {
            rejected.push_back(business.clone());
            env.storage()
                .instance()
                .set(&DataKey::RejectedBusinesses, &rejected);
        }
    }

    fn remove_from_verified_businesses(env: &Env, business: &Address) {
        let mut verified = Self::get_verified_businesses(env);
        if let Some(pos) = verified.iter().position(|a| a == *business) {
            verified.remove(pos as u32);
            env.storage()
                .instance()
                .set(&DataKey::VerifiedBusinesses, &verified);
        }
    }

    fn remove_from_pending_businesses(env: &Env, business: &Address) {
        let mut pending = Self::get_pending_businesses(env);
        if let Some(pos) = pending.iter().position(|a| a == *business) {
            pending.remove(pos as u32);
            env.storage()
                .instance()
                .set(&DataKey::PendingBusinesses, &pending);
        }
    }

    fn remove_from_rejected_businesses(env: &Env, business: &Address) {
        let mut rejected = Self::get_rejected_businesses(env);
        if let Some(pos) = rejected.iter().position(|a| a == *business) {
            rejected.remove(pos as u32);
            env.storage()
                .instance()
                .set(&DataKey::RejectedBusinesses, &rejected);
        }
    }
}

pub struct InvestorVerificationStorage;
impl InvestorVerificationStorage {
    pub fn store_verification(env: &Env, v: &crate::verification::InvestorVerification) {
        env.storage()
            .instance()
            .set(&DataKey::InvestorVerification(v.investor.clone()), v);

        match v.status {
            crate::verification::BusinessVerificationStatus::Verified => {
                Self::add_to_verified_investors(env, &v.investor);
            }
            crate::verification::BusinessVerificationStatus::Pending => {
                Self::add_to_pending_investors(env, &v.investor);
            }
            crate::verification::BusinessVerificationStatus::Rejected => {
                Self::add_to_rejected_investors(env, &v.investor);
            }
        }
    }

    pub fn get_verification(
        env: &Env,
        investor: &Address,
    ) -> Option<crate::verification::InvestorVerification> {
        Self::get(env, investor)
    }

    pub fn get(env: &Env, investor: &Address) -> Option<crate::verification::InvestorVerification> {
        env.storage()
            .instance()
            .get(&DataKey::InvestorVerification(investor.clone()))
    }

    pub fn update_verification(
        env: &Env,
        verification: &crate::verification::InvestorVerification,
    ) {
        let old_verification = Self::get_verification(env, &verification.investor);
        if let Some(old) = old_verification {
            match old.status {
                crate::verification::BusinessVerificationStatus::Verified => {
                    Self::remove_from_verified_investors(env, &verification.investor);
                }
                crate::verification::BusinessVerificationStatus::Pending => {
                    Self::remove_from_pending_investors(env, &verification.investor);
                }
                crate::verification::BusinessVerificationStatus::Rejected => {
                    Self::remove_from_rejected_investors(env, &verification.investor);
                }
            }
        }
        Self::store_verification(env, verification);
    }

    pub fn is_investor_verified(env: &Env, investor: &Address) -> bool {
        Self::get_verification(env, investor)
            .map(|v| v.status == crate::verification::BusinessVerificationStatus::Verified)
            .unwrap_or(false)
    }

    pub fn get_verified_investors(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::VerifiedInvestors)
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_pending_investors(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::PendingInvestors)
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_rejected_investors(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::RejectedInvestors)
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_investors_by_tier(
        env: &Env,
        tier: crate::verification::InvestorTier,
    ) -> Vec<Address> {
        let verified = Self::get_verified_investors(env);
        let mut result = Vec::new(env);
        for investor in verified.iter() {
            if let Some(v) = Self::get_verification(env, &investor) {
                if v.tier == tier {
                    result.push_back(investor);
                }
            }
        }
        result
    }

    pub fn get_investors_by_risk_level(
        env: &Env,
        risk_level: crate::verification::InvestorRiskLevel,
    ) -> Vec<Address> {
        let verified = Self::get_verified_investors(env);
        let mut result = Vec::new(env);
        for investor in verified.iter() {
            if let Some(v) = Self::get_verification(env, &investor) {
                if v.risk_level == risk_level {
                    result.push_back(investor);
                }
            }
        }
        result
    }

    fn add_to_verified_investors(env: &Env, investor: &Address) {
        let mut verified = Self::get_verified_investors(env);
        if !verified.contains(investor) {
            verified.push_back(investor.clone());
            env.storage()
                .instance()
                .set(&DataKey::VerifiedInvestors, &verified);
        }
    }

    fn add_to_pending_investors(env: &Env, investor: &Address) {
        let mut pending = Self::get_pending_investors(env);
        if !pending.contains(investor) {
            pending.push_back(investor.clone());
            env.storage()
                .instance()
                .set(&DataKey::PendingInvestors, &pending);
        }
    }

    fn add_to_rejected_investors(env: &Env, investor: &Address) {
        let mut rejected = Self::get_rejected_investors(env);
        if !rejected.contains(investor) {
            rejected.push_back(investor.clone());
            env.storage()
                .instance()
                .set(&DataKey::RejectedInvestors, &rejected);
        }
    }

    fn remove_from_verified_investors(env: &Env, investor: &Address) {
        let mut verified = Self::get_verified_investors(env);
        if let Some(pos) = verified.iter().position(|a| a == *investor) {
            verified.remove(pos as u32);
            env.storage()
                .instance()
                .set(&DataKey::VerifiedInvestors, &verified);
        }
    }

    fn remove_from_pending_investors(env: &Env, investor: &Address) {
        let mut pending = Self::get_pending_investors(env);
        if let Some(pos) = pending.iter().position(|a| a == *investor) {
            pending.remove(pos as u32);
            env.storage()
                .instance()
                .set(&DataKey::PendingInvestors, &pending);
        }
    }

    fn remove_from_rejected_investors(env: &Env, investor: &Address) {
        let mut rejected = Self::get_rejected_investors(env);
        if let Some(pos) = rejected.iter().position(|a| a == *investor) {
            rejected.remove(pos as u32);
            env.storage()
                .instance()
                .set(&DataKey::RejectedInvestors, &rejected);
        }
    }
}

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
