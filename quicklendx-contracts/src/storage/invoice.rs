//! Invoice storage operations, caching, and secondary index maintenance.

use soroban_sdk::{
    contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec,
};

use crate::errors::QuickLendXError;
use crate::protocol_limits;
use crate::types::{
    BusinessFreezeReason, FreezeInfo, InvestorFreezeInfo, Invoice,
    InvoiceCategory, InvoiceLock, InvoiceStatus, PruneReport, RebuildReport,
};
use super::{
    bump_persistent, extend_instance_ttl, extend_persistent_ttl, persistent_get, persistent_set,
    DataKey, Indexes, StorageKeys, StorageIntegrityAudit, LOCK_TIME_LIMIT_SECONDS,
};

/// Selects one secondary invoice index for bounded integrity cleanup.
#[derive(Clone)]
#[contracttype]
pub enum InvoiceIndex {
    Business(Address),
    Status(InvoiceStatus),
    Customer(String),
    TaxId(String),
    Tag(String),
    Category(InvoiceCategory),
}


impl StorageKeys {
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"inv_count"` resets the invoice counter on all deployed contracts.
    pub fn invoice_count() -> Symbol {
        symbol_short!("inv_count")
    }
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"biz_def_h"` resets the business default history counters.
    pub fn business_default_history(business: &Address) -> (Symbol, Address) {
        (symbol_short!("biz_def_h"), business.clone())
    }
}

impl Indexes {
    /// Returns the persistent storage key for the invoice list owned by a business.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"inv_bus"` orphans all per-business invoice indexes.
    pub fn invoices_by_business(business: &Address) -> (Symbol, Address) {
        (symbol_short!("inv_bus"), business.clone())
    }

    /// Returns the persistent storage key for the invoice list in a given status bucket.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming any status symbol or `"inv_st"` orphans that status index.
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

    /// Returns the persistent storage key for the invoice list indexed by customer name.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"inv_cust"` orphans all customer-name metadata indexes.
    pub fn invoices_by_customer(customer_name: &String) -> (Symbol, String) {
        (symbol_short!("inv_cust"), customer_name.clone())
    }

    /// Returns the persistent storage key for the invoice list indexed by tax ID.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"inv_taxid"` orphans all tax-ID metadata indexes.
    pub fn invoices_by_tax_id(tax_id: &String) -> (Symbol, String) {
        (symbol_short!("inv_taxid"), tax_id.clone())
    }

    /// Returns the persistent storage key for the invoice list indexed by a tag string.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming `"inv_tag"` orphans all tag indexes.
    pub fn invoices_by_tag(tag: &String) -> (Symbol, String) {
        (symbol_short!("inv_tag"), tag.clone())
    }

    /// Returns the persistent storage key for the invoice list in a given category bucket.
    ///
    /// **Storage class**: Persistent  
    /// **BREAKING**: Renaming any category symbol or `"inv_cat"` orphans that category index.
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
    fn business_generation_key(business: &Address) -> (Symbol, Address) {
        (symbol_short!("biz_gen"), business.clone())
    }

    pub fn get_business_generation(env: &Env, business: &Address) -> u64 {
        let key = Self::business_generation_key(business);
        persistent_get(env, &key).unwrap_or(0)
        let generation = env.storage().persistent().get(&key).unwrap_or(0);
        if env.storage().persistent().has(&key) {
            extend_persistent_ttl(env, &key);
        }
        generation
    }

    fn get_business_cursor_generation(
        env: &Env,
        business: &Address,
        status_filter: Option<InvoiceStatus>,
    ) -> u64 {
        let status_tag = match status_filter {
            None => 0,
            Some(InvoiceStatus::Pending) => 1,
            Some(InvoiceStatus::Verified) => 2,
            Some(InvoiceStatus::Funded) => 3,
            Some(InvoiceStatus::Paid) => 4,
            Some(InvoiceStatus::Defaulted) => 5,
            Some(InvoiceStatus::Cancelled) => 6,
            Some(InvoiceStatus::Refunded) => 7,
        };
        let mut preimage = soroban_sdk::Bytes::from_slice(env, b"business_invoice_cursor_v1");
        preimage.append(&business.to_string().to_bytes());
        preimage.append(&soroban_sdk::Bytes::from_array(env, &[status_tag]));
        preimage.append(&soroban_sdk::Bytes::from_array(
            env,
            &Self::get_business_generation(env, business).to_be_bytes(),
        ));
        let digest = env.crypto().sha256(&preimage).to_array();
        u64::from_be_bytes([
            digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
        ])
    }

    pub fn bump_business_generation(env: &Env, business: &Address) {
        let key = Self::business_generation_key(business);
        let next = Self::get_business_generation(env, business).saturating_add(1);
        env.storage().persistent().set(&key, &next);
        extend_persistent_ttl(env, &key);
    }

    fn status_generation_key(status: InvoiceStatus) -> (Symbol, Symbol) {
        let status_symbol = match status {
            InvoiceStatus::Pending => symbol_short!("g_pending"),
            InvoiceStatus::Verified => symbol_short!("g_verif"),
            InvoiceStatus::Funded => symbol_short!("g_funded"),
            InvoiceStatus::Paid => symbol_short!("g_paid"),
            InvoiceStatus::Defaulted => symbol_short!("g_default"),
            InvoiceStatus::Cancelled => symbol_short!("g_cancel"),
            InvoiceStatus::Refunded => symbol_short!("g_refund"),
        };
        (symbol_short!("st_gen"), status_symbol)
    }

    pub fn get_status_generation(env: &Env, status: InvoiceStatus) -> u64 {
        let key = Self::status_generation_key(status);
        persistent_get(env, &key).unwrap_or(0)
    }

    pub fn bump_status_generation(env: &Env, status: InvoiceStatus) {
        let key = Self::status_generation_key(status);
        let next = Self::get_status_generation(env, status).saturating_add(1);
        env.storage().persistent().set(&key, &next);
        extend_persistent_ttl(env, &key);
    }

    fn raw_index_entries(env: &Env, index: &InvoiceIndex) -> Vec<BytesN<32>> {
        match index {
            InvoiceIndex::Business(business) => {
                let key = Indexes::invoices_by_business(business);
                persistent_get(env, &key).unwrap_or(Vec::new(env))
            }
            InvoiceIndex::Status(status) => {
                let key = Indexes::invoices_by_status(*status);
                persistent_get(env, &key).unwrap_or(Vec::new(env))
            }
            InvoiceIndex::Customer(name) => {
                let key = Indexes::invoices_by_customer(name);
                persistent_get(env, &key).unwrap_or(Vec::new(env))
            }
            InvoiceIndex::TaxId(tax_id) => {
                let key = Indexes::invoices_by_tax_id(tax_id);
                persistent_get(env, &key).unwrap_or(Vec::new(env))
            }
            InvoiceIndex::Tag(tag) => {
                let key = Indexes::invoices_by_tag(tag);
                persistent_get(env, &key).unwrap_or(Vec::new(env))
            }
            InvoiceIndex::Category(category) => {
                let key = Indexes::invoices_by_category(*category);
                persistent_get(env, &key).unwrap_or(Vec::new(env))
            }
        }
    }

    fn index_entries(env: &Env, index: &InvoiceIndex) -> Vec<BytesN<32>> {
        let mut valid = Vec::new(env);
        for id in Self::raw_index_entries(env, index).iter() {
            if Self::entry_matches(env, index, &id) && !valid.contains(&id) {
                valid.push_back(id);
            }
        }
        valid
    }

    fn entry_matches(env: &Env, index: &InvoiceIndex, id: &BytesN<32>) -> bool {
        let Some(invoice) = Self::get(env, id) else {
            return false;
        };
        match index {
            InvoiceIndex::Business(business) => invoice.business == *business,
            InvoiceIndex::Status(status) => invoice.status == *status,
            InvoiceIndex::Customer(name) => invoice.metadata_customer_name.as_ref() == Some(name),
            InvoiceIndex::TaxId(tax_id) => invoice.metadata_tax_id.as_ref() == Some(tax_id),
            InvoiceIndex::Tag(tag) => invoice.tags.iter().any(|candidate| candidate == *tag),
            InvoiceIndex::Category(category) => invoice.category == *category,
        }
    }

    /// Remove invalid entries from one secondary index in a bounded, replayable page.
    pub fn cleanup_index_page(
        env: &Env,
        index: &InvoiceIndex,
        offset: u32,
        limit: u32,
    ) -> crate::types::IndexCleanupReport {
        const MAX_INDEX_CLEANUP_PAGE: u32 = 100;
        let capped_limit = limit.min(MAX_INDEX_CLEANUP_PAGE);
        let mut entries = Self::raw_index_entries(env, index);
        let total = entries.len();
        let start = offset.min(total);
        let end = start.saturating_add(capped_limit).min(total);
        let mut removed = 0u32;
        let mut valid_seen = Vec::new(env);
        let mut position = start;
        let mut scanned = 0u32;
        while scanned < end.saturating_sub(start) && position < entries.len() {
            let Some(id) = entries.get(position) else {
                break;
            };
            let valid = Self::entry_matches(env, index, &id) && !valid_seen.contains(&id);
            if valid {
                valid_seen.push_back(id);
                position += 1;
            } else {
                entries.remove(position);
                removed = removed.saturating_add(1);
            }
            scanned += 1;
        }

        if removed > 0 {
            match index {
                InvoiceIndex::Business(business) => {
                    let key = Indexes::invoices_by_business(business);
                    env.storage().persistent().set(&key, &entries);
                    extend_persistent_ttl(env, &key);
                    Self::bump_business_generation(env, business);
                }
                InvoiceIndex::Status(status) => {
                    let key = Indexes::invoices_by_status(*status);
                    env.storage().persistent().set(&key, &entries);
                    extend_persistent_ttl(env, &key);
                    Self::bump_status_generation(env, *status);
                }
                InvoiceIndex::Customer(name) => {
                    let key = Indexes::invoices_by_customer(name);
                    env.storage().persistent().set(&key, &entries);
                    extend_persistent_ttl(env, &key);
                }
                InvoiceIndex::TaxId(tax_id) => {
                    let key = Indexes::invoices_by_tax_id(tax_id);
                    env.storage().persistent().set(&key, &entries);
                    extend_persistent_ttl(env, &key);
                }
                InvoiceIndex::Tag(tag) => {
                    let key = Indexes::invoices_by_tag(tag);
                    env.storage().persistent().set(&key, &entries);
                    extend_persistent_ttl(env, &key);
                }
                InvoiceIndex::Category(category) => {
                    let key = Indexes::invoices_by_category(*category);
                    env.storage().persistent().set(&key, &entries);
                    extend_persistent_ttl(env, &key);
                }
            };
        }

        crate::types::IndexCleanupReport {
            scanned,
            removed,
            next_offset: if removed > 0 { start } else { end },
        }
    }

    /// Store an invoice and update all its secondary indexes.
    pub fn store(env: &Env, invoice: &Invoice) {
        crate::assert_view_only!(env);
        let key = DataKey::Invoice(invoice.id.clone());
        env.storage().persistent().set(&key, invoice);
        extend_persistent_ttl(env, &key);
        Self::add_to_business_index(env, &invoice.business, &invoice.id);
        Self::add_to_status_index(env, invoice.status, &invoice.id);
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

    pub fn set_frozen(
        env: &Env,
        invoice_id: &BytesN<32>,
        frozen: bool,
        reason: Option<BusinessFreezeReason>,
    ) {
        let key = DataKey::FrozenInvoice(invoice_id.clone());
        if frozen {
            // Store the typed reason; callers must supply one when freezing.
            let r = reason.unwrap_or(BusinessFreezeReason::AdminAction);
            env.storage().persistent().set(&key, &r);
            extend_persistent_ttl(env, &key);
        } else {
            env.storage().persistent().remove(&key);
        }
    }

    pub fn set_invoice_lock(env: &Env, invoice_id: &BytesN<32>, lock: InvoiceLock) {
        let key = DataKey::FrozenInvoice(invoice_id.clone());
        if lock == InvoiceLock::None {
            env.storage().persistent().remove(&key);
        } else {
            env.storage().persistent().set(&key, &lock);
            extend_persistent_ttl(env, &key);
        }
    }

    pub fn get_invoice_lock(env: &Env, invoice_id: &BytesN<32>) -> InvoiceLock {
        let key = DataKey::FrozenInvoice(invoice_id.clone());
        persistent_get(env, &key).unwrap_or(InvoiceLock::None)
    }

    pub fn is_frozen(env: &Env, invoice_id: &BytesN<32>) -> bool {
        let key = DataKey::FrozenInvoice(invoice_id.clone());
        if let Some(lock) = env.storage().persistent().get::<_, InvoiceLock>(&key) {
            extend_persistent_ttl(env, &key);
            lock.is_locked()
        } else if env
            .storage()
            .persistent()
            .get::<_, BusinessFreezeReason>(&key)
            .is_some()
        {
            // Backward-compatible: a typed freeze reason also means frozen.
            extend_persistent_ttl(env, &key);
            true
        } else {
            false
        }
    }

    /// Guard that rejects actions on locks older than the time limit.
    ///
    /// Returns `InvoiceLockExpired` if the invoice has been frozen for longer
    /// than `LOCK_TIME_LIMIT_SECONDS` (30 days).
    pub fn require_lock_within_time_limit(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<(), QuickLendXError> {
        if let Some(freeze_info) = Self::get_freeze_info(env, invoice_id) {
            let current_time = env.ledger().timestamp();
            let lock_age = current_time.saturating_sub(freeze_info.frozen_at);
            if lock_age > LOCK_TIME_LIMIT_SECONDS {
                return Err(QuickLendXError::InvoiceLockExpired);
            }
        }
        Ok(())
    }

    pub fn set_freeze_info(env: &Env, invoice_id: &BytesN<32>, info: &FreezeInfo) {
        let key = DataKey::FreezeInfo(invoice_id.clone());
        env.storage().persistent().set(&key, info);
        extend_persistent_ttl(env, &key);
    }

    pub fn get_freeze_info(env: &Env, invoice_id: &BytesN<32>) -> Option<FreezeInfo> {
        let key = DataKey::FreezeInfo(invoice_id.clone());
        let result = env.storage().persistent().get::<_, FreezeInfo>(&key);
        if result.is_some() {
            extend_persistent_ttl(env, &key);
        }
        result
    }

    pub fn remove_freeze_info(env: &Env, invoice_id: &BytesN<32>) {
        let key = DataKey::FreezeInfo(invoice_id.clone());
        env.storage().persistent().remove(&key);
    }

    pub fn set_investor_freeze_info(env: &Env, investor: &Address, info: &InvestorFreezeInfo) {
        let key = DataKey::InvestorFreezeInfo(investor.clone());
        env.storage().persistent().set(&key, info);
        extend_persistent_ttl(env, &key);
    }

    pub fn get_investor_freeze_info(env: &Env, investor: &Address) -> Option<InvestorFreezeInfo> {
        let key = DataKey::InvestorFreezeInfo(investor.clone());
        let result = env
            .storage()
            .persistent()
            .get::<_, InvestorFreezeInfo>(&key);
        if result.is_some() {
            extend_persistent_ttl(env, &key);
        }
        result
    }

    pub fn remove_investor_freeze_info(env: &Env, investor: &Address) {
        let key = DataKey::InvestorFreezeInfo(investor.clone());
        env.storage().persistent().remove(&key);
    }

    pub fn get_by_business(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        Self::index_entries(env, &InvoiceIndex::Business(business.clone()))
    }

    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        Self::get_by_business(env, business)
    }

    /// Return business invoices in descending creation-time order using an
    /// opaque, generation-bound cursor.
    pub fn get_business_invoices_paged(
        env: &Env,
        business: &Address,
        status_filter: Option<InvoiceStatus>,
        cursor: Option<soroban_sdk::String>,
        limit: u32,
    ) -> Result<crate::types::InvoicePage, crate::errors::QuickLendXError> {
        let generation = Self::get_business_cursor_generation(env, business, status_filter);
        let offset = match cursor {
            Some(raw) => {
                let decoded = crate::pagination::decode_soroban_cursor(&raw)?;
                decoded.require_stable(generation)?;
                decoded.offset
            }
            None => 0,
        };

        let mut ordered = Vec::new(env);
        for id in Self::get_business_invoices(env, business).iter() {
            let Some(invoice) = Self::get(env, &id) else {
                continue;
            };
            if status_filter.is_some_and(|status| invoice.status != status) {
                continue;
            }

            let mut insertion_index = ordered.len();
            ordered.push_back(id.clone());
            while insertion_index > 0 {
                let previous_id = ordered.get(insertion_index - 1).unwrap();
                let previous_created_at = Self::get(env, &previous_id)
                    .map(|previous| previous.created_at)
                    .unwrap_or(0);
                if previous_created_at >= invoice.created_at {
                    break;
                }
                ordered.set(insertion_index, previous_id);
                insertion_index -= 1;
            }
            ordered.set(insertion_index, id);
        }

        let total_count = ordered.len();
        let (start, end) = crate::pagination::calculate_safe_bounds(offset, limit, total_count);
        let mut items = Vec::new(env);
        for index in start..end {
            if let Some(id) = ordered.get(index) {
                items.push_back(id);
            }
        }

        let capped_limit = crate::pagination::cap_query_limit(limit);
        let has_more = capped_limit > 0 && offset.saturating_add(capped_limit) < total_count;
        let next_cursor = if has_more {
            Some(crate::pagination::encode_soroban_cursor(
                env,
                crate::pagination::Cursor::new(start.saturating_add(capped_limit), generation),
            ))
        } else {
            None
        };

        Ok(crate::types::InvoicePage {
            items,
            total_count,
            has_more,
            generation,
            next_cursor,
        })
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
        Self::index_entries(env, &InvoiceIndex::Status(status))
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

    /// Returns the optional absolute per-investor position cap for an invoice.
    ///
    /// `None` means the invoice is uncapped (only face value / protocol limits apply).
    pub fn get_per_investor_position_cap(env: &Env, invoice_id: &BytesN<32>) -> Option<i128> {
        let key = DataKey::PerInvestorPositionCap(invoice_id.clone());
        persistent_get(env, &key)
    }

    /// Sets or clears the absolute per-investor position cap for an invoice.
    ///
    /// Passing `None` removes the cap (uncapped). Callers must validate
    /// `cap > 0 && cap <= invoice.amount` before storing `Some(cap)`.
    pub fn set_per_investor_position_cap(env: &Env, invoice_id: &BytesN<32>, cap: Option<i128>) {
        crate::assert_view_only!(env);
        let key = DataKey::PerInvestorPositionCap(invoice_id.clone());
        match cap {
            Some(value) => {
                env.storage().persistent().set(&key, &value);
                extend_persistent_ttl(env, &key);
            }
            None => {
                env.storage().persistent().remove(&key);
            }
        }
    }

    pub fn update(env: &Env, invoice: &Invoice) {
        crate::assert_view_only!(env);
        if let Some(old) = Self::get(env, &invoice.id) {
            if old.status != invoice.status {
                Self::remove_from_status_index(env, old.status, &invoice.id);
                Self::add_to_status_index(env, invoice.status, &invoice.id);
                Self::bump_business_generation(env, &invoice.business);
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
        env.storage().persistent().set(&key, invoice);
        extend_persistent_ttl(env, &key);
    }

    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        Self::update(env, invoice)
    }

    pub fn next_count(env: &Env) -> u64 {
        let key = StorageKeys::invoice_count();
        let current: u64 = persistent_get(env, &key).unwrap_or(0);
        let next = current.saturating_add(1);
        persistent_set(env, &key, &next);
        next
    }

    pub fn get_total_count(env: &Env) -> u64 {
        let key = StorageKeys::invoice_count();
        persistent_get(env, &key).unwrap_or(0)
    }

    pub fn set_total_count(env: &Env, count: u32) {
        let key = StorageKeys::invoice_count();
        persistent_set(env, &key, &(count as u64));
    }

    pub fn delete_invoice(env: &Env, invoice_id: &BytesN<32>) {
        crate::assert_view_only!(env);
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

    pub fn clear_all(env: &Env) -> Result<(), QuickLendXError> {
        crate::governance::require_no_open_governance_proposal(env)?;
        let ids = Self::get_all_invoice_ids(env);
        for id in ids.iter() {
            Self::delete_invoice(env, &id);
        }
        StorageManager::clear_all_mappings(env)?;
        Ok(())
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
                    .is_some_and(|rating| rating > threshold)
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
        let index = InvoiceIndex::Business(business.clone());
        let mut invoices = Self::raw_index_entries(env, &index);

        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());

            let key = Indexes::invoices_by_business(business);
            env.storage().persistent().set(&key, &invoices);
            extend_persistent_ttl(env, &key);
            Self::bump_business_generation(env, business);
        }
    }

    fn remove_from_business_index(env: &Env, business: &Address, invoice_id: &BytesN<32>) {
        let index = InvoiceIndex::Business(business.clone());
        let mut invoices = Self::raw_index_entries(env, &index);

        if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
            invoices.remove(pos as u32);

            let key = Indexes::invoices_by_business(business);
            env.storage().persistent().set(&key, &invoices);
            extend_persistent_ttl(env, &key);
            Self::bump_business_generation(env, business);
        }
    }

    fn add_to_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let index = InvoiceIndex::Status(status);
        let mut invoices = Self::raw_index_entries(env, &index);

        if !invoices.contains(invoice_id) {
            invoices.push_back(invoice_id.clone());

            let key = Indexes::invoices_by_status(status);
            env.storage().persistent().set(&key, &invoices);
            extend_persistent_ttl(env, &key);
            Self::bump_status_generation(env, status);
        }
    }

    fn remove_from_status_index(env: &Env, status: InvoiceStatus, invoice_id: &BytesN<32>) {
        let index = InvoiceIndex::Status(status);
        let mut invoices = Self::raw_index_entries(env, &index);

        if let Some(pos) = invoices.iter().position(|id| id == *invoice_id) {
            invoices.remove(pos as u32);

            let key = Indexes::invoices_by_status(status);
            env.storage().persistent().set(&key, &invoices);
            extend_persistent_ttl(env, &key);
            Self::bump_status_generation(env, status);
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
        let key = Indexes::invoices_by_category(*category);
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
        let key = Indexes::invoices_by_category(*category);
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
        Self::index_entries(env, &InvoiceIndex::Customer(customer_name.clone()))
    }

    pub fn get_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>> {
        Self::get_invoices_by_customer(env, customer_name)
    }

    pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        Self::index_entries(env, &InvoiceIndex::TaxId(tax_id.clone()))
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

    /// Efficiently retrieves invoice IDs for a category directly from the category index.
    /// Unlike `get_invoices_by_category`, this reads the index without scanning all invoices.
    pub fn get_invoices_by_category_from_index(
        env: &Env,
        category: &InvoiceCategory,
    ) -> Vec<BytesN<32>> {
        Self::index_entries(env, &InvoiceIndex::Category(*category))
    }

    /// Efficiently counts invoices for a category directly from the category index.
    /// This is the preferred method for counting and is bounded by the index size.
    pub fn get_invoice_count_by_category_from_index(env: &Env, category: &InvoiceCategory) -> u32 {
        Self::get_invoices_by_category_from_index(env, category).len()
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
        Self::index_entries(env, &InvoiceIndex::Tag(tag.clone()))
    }

    /// Look up invoices matching every tag in `tags` (AND logic).
    ///
    /// # Resource bound
    /// `tags.len()` is rejected above [`crate::verification::MAX_INVOICE_TAG_COUNT`]
    /// *before* any index work begins. No invoice can ever be stored with more
    /// tags than that cap (enforced at creation by
    /// [`crate::verification::validate_invoice_tags`]), so a query requesting
    /// more tags than the cap can never match anything — it can only force the
    /// contract to redo the tag-index scan below once per extra tag. Without
    /// this check, an unauthenticated caller could pass an arbitrarily long
    /// `tags` vector and multiply the cost of this call by its length.
    pub fn get_invoices_by_tags(
        env: &Env,
        tags: &Vec<String>,
    ) -> Result<Vec<BytesN<32>>, QuickLendXError> {
        if tags.len() > crate::verification::MAX_INVOICE_TAG_COUNT {
            return Err(QuickLendXError::TagLimitExceeded);
        }
        if tags.is_empty() {
            return Ok(Vec::new(env));
        }
        let mut result = Vec::new(env);
        let first_tag = tags.get(0).ok_or(QuickLendXError::InvalidTag)?;
        let first_ids = Self::get_invoices_by_tag(env, &first_tag);

        for id in first_ids.iter() {
            let mut all_match = true;
            for i in 1..tags.len() {
                let tag = tags.get(i).ok_or(QuickLendXError::InvalidTag)?;
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
        Ok(result)
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

pub struct StorageReadCache {
    cached_id: [u8; 32],
    cached_invoice: Option<Invoice>,
    has_cached: bool,
}

impl StorageReadCache {
    pub fn new() -> Self {
        Self {
            cached_id: [0u8; 32],
            cached_invoice: None,
            has_cached: false,
        }
    }

    /// Return a cached invoice if already fetched in this call, otherwise read
    /// from persistent storage and cache the result.
    pub fn get_invoice(&mut self, env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        if self.has_cached && invoice_id.to_array() == self.cached_id {
            return self.cached_invoice.clone();
        }
        let invoice = InvoiceStorage::get_invoice(env, invoice_id);
        self.cached_invoice = invoice.clone();
        self.has_cached = true;
        self.cached_id = invoice_id.to_array();
        invoice
    }

    /// Invalidate the cache when the underlying storage has been updated so
    /// that the next `get_invoice` performs a fresh read from storage.
    pub fn invalidate_invoice(&mut self, invoice_id: &BytesN<32>) {
        if self.has_cached && invoice_id.to_array() == self.cached_id {
            self.has_cached = false;
        }
    }
}

/// Comprehensive integrity audit for protocol storage indexes.
///
/// This helper provides deep inspection of secondary indexes to ensure no
/// orphan IDs exist and that all records are mutually consistent across
/// different indexing strategies (status, owner, metadata).

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
            let ids = InvoiceStorage::get_by_status(env, status);
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
                let category_ids = InvoiceStorage::get_by_category(env, invoice.category);
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
        let capped = if limit > MAX_REBUILD_PAGE {
            MAX_REBUILD_PAGE
        } else {
            limit
        };

        let all_ids = Self::get_all_invoice_ids(env);
        let total = all_ids.len();

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

    /// Prune terminal-state invoices whose terminal timestamp is older than
    /// `older_than_secs` from the current ledger timestamp.
    ///
    /// Only invoices in a terminal status (`Paid`, `Defaulted`, `Cancelled`,
    /// `Refunded`) are eligible. For `Paid` invoices the terminal timestamp
    /// is `settled_at`; for other terminal statuses it falls back to
    /// `created_at`. Invoices in `Pending`, `Verified`, or `Funded` status
    /// are never pruned regardless of age.
    ///
    /// The operation is paginated via `offset`/`limit` (capped at 100 per
    /// page) and removes each pruned invoice from all secondary indexes
    /// (status, business, customer, tax_id, tag, category) and from primary
    /// persistent storage via [`delete_invoice`](Self::delete_invoice).
    ///
    /// # Resumability
    /// Pass the `next_offset` from the returned `PruneReport` as `offset`
    /// on the next call. Stop when `next_offset` stops advancing (last page).
    ///
    /// # Returns
    /// A `PruneReport` containing:
    /// * `scanned`   — number of invoice IDs examined in this page.
    /// * `pruned`    — number of invoices actually deleted.
    /// * `next_offset` — offset for the next call.
    pub fn prune_terminal_invoices_page(
        env: &Env,
        older_than_secs: u64,
        offset: u32,
        limit: u32,
    ) -> PruneReport {
        const MAX_PRUNE_PAGE: u32 = 100;
        let capped = if limit > MAX_PRUNE_PAGE {
            MAX_PRUNE_PAGE
        } else {
            limit
        };

        let now = env.ledger().timestamp();
        let all_ids = Self::get_all_invoice_ids(env);
        let total = all_ids.len();

        let start = offset.min(total);
        let end = start.saturating_add(capped).min(total);

        let mut pruned: u32 = 0;
        let mut i = start;
        while i < end {
            if let Some(id) = all_ids.get(i) {
                if let Some(invoice) = Self::get(env, &id) {
                    if invoice.status.is_terminal() {
                        let terminal_ts = invoice.settled_at.unwrap_or(invoice.created_at);
                        if terminal_ts.saturating_add(older_than_secs) < now {
                            Self::delete_invoice(env, &id);
                            pruned = pruned.saturating_add(1);
                        }
                    }
                }
            }
            i = i.saturating_add(1);
        }

        PruneReport {
            scanned: end.saturating_sub(start),
            pruned,
            next_offset: end,
        }
    }
}

#[cfg(all(test, feature = "legacy-tests"))]
#[cfg(test)]
mod test_storage_read_cache {
    use super::*;
    use crate::types::{
        Dispute, DisputeResolution, DisputeStatus, Invoice, InvoiceCategory, InvoiceRating,
        InvoiceStatus, PaymentRecord,
    };

    use soroban_sdk::{testutils::Address as _, Address, Env};

    fn create_sample_invoice(env: &Env, id: &BytesN<32>, business: &Address) -> Invoice {
        Invoice {
            id: id.clone(),
            business: business.clone(),
            amount: 1000,
            currency: Address::generate(env),
            due_date: env.ledger().timestamp() + 86400,
            status: InvoiceStatus::Funded,
            created_at: env.ledger().timestamp(),
            description: String::from_str(env, "test invoice"),
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(env),
            category: InvoiceCategory::Services,
            tags: Vec::new(env),
            funded_amount: 1000,
            funded_at: Some(env.ledger().timestamp()),
            investor: Some(business.clone()),
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(env),
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: Address::generate(env),
                created_at: 0,
                reason: String::from_str(env, ""),
                evidence: String::from_str(env, ""),
                resolution: String::from_str(env, ""),
                resolved_by: Address::generate(env),
                resolved_at: 0,
                resolution_outcome: DisputeResolution::None,
                votes: Vec::new(env),
            },
            total_paid: 0,
            payment_history: Vec::new(env),
            origination_fee_bps: None,
            late_payment_penalty_bps: None,
            early_payment_discount_bps: None,
        }
    }

    #[test]
    fn test_cache_hit_returns_same_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let business = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[1; 32]);
        let invoice = create_sample_invoice(&env, &invoice_id, &business);

        InvoiceStorage::store_invoice(&env, &invoice);

        let mut cache = StorageReadCache::new();
        let first = cache.get_invoice(&env, &invoice_id);
        assert!(first.is_some());
        assert_eq!(first.as_ref().unwrap().amount, 1000);

        // Second read should hit the cache and return the same value
        // without an additional storage call.
        let second = cache.get_invoice(&env, &invoice_id);
        assert_eq!(first, second);
    }

    #[test]
    fn test_cache_miss_after_invalidate() {
        let env = Env::default();
        env.mock_all_auths();
        let business = Address::generate(&env);
        let invoice_id = BytesN::from_array(&env, &[1; 32]);
        let mut invoice = create_sample_invoice(&env, &invoice_id, &business);

        InvoiceStorage::store_invoice(&env, &invoice);

        let mut cache = StorageReadCache::new();

        // Seed cache
        let cached = cache.get_invoice(&env, &invoice_id);
        assert!(cached.is_some());

        // Update the stored invoice
        invoice.amount = 2000;
        invoice.total_paid = 2000;
        InvoiceStorage::update_invoice(&env, &invoice);

        // Without invalidation the cache would return stale data (amount=1000).
        // After invalidation it must do a fresh read.
        cache.invalidate_invoice(&invoice_id);
        let fresh = cache.get_invoice(&env, &invoice_id).unwrap();
        assert_eq!(fresh.amount, 2000);
    }

    #[test]
    fn test_cache_different_keys_independent() {
        let env = Env::default();
        env.mock_all_auths();
        let business = Address::generate(&env);

        let id_a = BytesN::from_array(&env, &[1; 32]);
        let id_b = BytesN::from_array(&env, &[2; 32]);
        let inv_a = create_sample_invoice(&env, &id_a, &business);
        let mut inv_b = create_sample_invoice(&env, &id_b, &business);
        inv_b.amount = 500;

        InvoiceStorage::store_invoice(&env, &inv_a);
        InvoiceStorage::store_invoice(&env, &inv_b);

        let mut cache = StorageReadCache::new();

        let a1 = cache.get_invoice(&env, &id_a).unwrap();
        assert_eq!(a1.amount, 1000);

        let b1 = cache.get_invoice(&env, &id_b).unwrap();
        assert_eq!(b1.amount, 500);

        // Invalidate A — B should still be cached
        cache.invalidate_invoice(&id_a);

        let a2 = cache.get_invoice(&env, &id_a).unwrap();
        assert_eq!(a2.amount, 1000); // fresh read, same value
    }
}
