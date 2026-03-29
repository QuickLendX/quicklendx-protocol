use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Vec};
use crate::types::{Invoice, InvoiceMetadata, InvoiceCategory, InvoiceStatus};
use crate::errors::QuickLendXError;

// ... (Normalize tag function can stay if still needed, but check if it's in verification)

pub struct InvoiceStorage;

impl InvoiceStorage {
    pub fn metadata_customer_key(_env: &Env, name: &String) -> (soroban_sdk::Symbol, String) {
        (soroban_sdk::symbol_short!("met_cust"), name.clone())
    }

    pub fn metadata_tax_key(_env: &Env, tax_id: &String) -> (soroban_sdk::Symbol, String) {
        (soroban_sdk::symbol_short!("met_tax"), tax_id.clone())
    }

    pub fn add_metadata_indexes(env: &Env, invoice: &Invoice) {
        if let Some(name) = &invoice.metadata_customer_name {
            let key = Self::metadata_customer_key(env, name);
            Self::add_to_metadata_index(env, &key, &invoice.id);
        }
        if let Some(tax) = &invoice.metadata_tax_id {
            let key = Self::metadata_tax_key(env, tax);
            Self::add_to_metadata_index(env, &key, &invoice.id);
        }
    }

    pub fn remove_metadata_indexes(env: &Env, metadata: &InvoiceMetadata, invoice_id: &BytesN<32>) {
        let ck = Self::metadata_customer_key(env, &metadata.customer_name);
        Self::remove_from_metadata_index(env, &ck, invoice_id);
        let tk = Self::metadata_tax_key(env, &metadata.tax_id);
        Self::remove_from_metadata_index(env, &tk, invoice_id);
    }

    pub fn store_invoice(env: &Env, invoice: &Invoice) {
        env.storage().persistent().set(&invoice.id, invoice);
        Self::add_to_status_index(env, &invoice.status, &invoice.id);
        Self::add_to_business_index(env, &invoice.business, &invoice.id);
    }

    pub fn get_invoice(env: &Env, id: &BytesN<32>) -> Option<Invoice> {
        env.storage().persistent().get(id)
    }

    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        env.storage().persistent().set(&invoice.id, invoice);
    }

    pub fn add_to_status_index(env: &Env, status: &InvoiceStatus, id: &BytesN<32>) {
        let key = (symbol_short!("inv_stat"), status.clone());
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| &existing == id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn remove_from_status_index(env: &Env, status: &InvoiceStatus, id: &BytesN<32>) {
        let key = (symbol_short!("inv_stat"), status.clone());
        if let Some(ids) = env.storage().persistent().get::<_, Vec<BytesN<32>>>(&key) {
            let mut updated = Vec::new(env);
            for existing in ids.iter() {
                if &existing != id {
                    updated.push_back(existing);
                }
            }
            env.storage().persistent().set(&key, &updated);
        }
    }

    pub fn add_to_business_index(env: &Env, business: &Address, id: &BytesN<32>) {
        let key = (symbol_short!("inv_bus"), business.clone());
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| &existing == id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(&key, &ids);
        }
    }

    pub fn add_to_metadata_index(env: &Env, key: &(soroban_sdk::Symbol, String), id: &BytesN<32>) {
        let mut ids: Vec<BytesN<32>> = env.storage().persistent().get(key).unwrap_or_else(|| Vec::new(env));
        if !ids.iter().any(|existing| &existing == id) {
            ids.push_back(id.clone());
            env.storage().persistent().set(key, &ids);
        }
    }

    pub fn remove_from_metadata_index(env: &Env, key: &(soroban_sdk::Symbol, String), id: &BytesN<32>) {
        if let Some(ids) = env.storage().persistent().get::<_, Vec<BytesN<32>>>(key) {
            let mut updated = Vec::new(env);
            for existing in ids.iter() {
                if &existing != id {
                    updated.push_back(existing);
                }
            }
            env.storage().persistent().set(key, &updated);
        }
    }

    pub fn get_invoices_by_status(env: &Env, status: &InvoiceStatus) -> Vec<BytesN<32>> {
        let key = (symbol_short!("inv_stat"), status.clone());
        env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        let key = (symbol_short!("inv_bus"), business.clone());
        env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_invoices_by_customer(env: &Env, name: &String) -> Vec<BytesN<32>> {
        let key = Self::metadata_customer_key(env, name);
        env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        let key = Self::metadata_tax_key(env, tax_id);
        env.storage().persistent().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    pub fn count_active_business_invoices(env: &Env, business: &Address) -> u32 {
        let pending = Self::get_invoices_by_status(env, &InvoiceStatus::Pending);
        let verified = Self::get_invoices_by_status(env, &InvoiceStatus::Verified);
        let funded = Self::get_invoices_by_status(env, &InvoiceStatus::Funded);
        
        let mut count = 0u32;
        let bus_invoices = Self::get_business_invoices(env, business);
        
        for id in bus_invoices.iter() {
            if pending.iter().any(|p| p == id) || verified.iter().any(|v| v == id) || funded.iter().any(|f| f == id) {
                count = count.saturating_add(1);
            }
        }
        count
    }

    pub fn remove_from_status_invoices(env: &Env, status: &InvoiceStatus, id: &BytesN<32>) {
        Self::remove_from_status_index(env, status, id);
    }

    pub fn add_to_status_invoices(env: &Env, status: &InvoiceStatus, id: &BytesN<32>) {
        Self::add_to_status_index(env, status, id);
    }
}