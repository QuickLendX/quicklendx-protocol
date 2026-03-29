// ... (Normalize tag function remains same)

impl Invoice {
    // ... (other methods)

    pub fn remove_tag(&mut self, env: &Env, tag: String) -> Result<(), QuickLendXError> {
        self.business.require_auth();
        let normalized = normalize_tag(env, &tag)?;
        
        let mut new_tags = Vec::new(env); // Use passed-in env
        let mut found = false;

        for existing_tag in self.tags.iter() {
            if existing_tag != normalized {
                new_tags.push_back(existing_tag);
            } else {
                found = true;
            }
        }

        if !found { return Err(QuickLendXError::InvalidTag); }

        self.tags = new_tags;
        InvoiceStorage::remove_tag_index(env, &normalized, &self.id);
        Ok(())
    }
}

impl InvoiceStorage {
    pub fn metadata_customer_key(env: &Env, name: &String) -> (soroban_sdk::Symbol, String) {
        (soroban_sdk::symbol_short!("met_cust"), name.clone())
    }

    pub fn metadata_tax_key(env: &Env, tax_id: &String) -> (soroban_sdk::Symbol, String) {
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
    
    // ... (rest of methods like get_invoice, store_invoice, etc.)
}