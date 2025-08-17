use soroban_sdk::{contracttype, symbol_short, vec, Address, BytesN, Env, Map, String, Vec};

/// Invoice status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Pending,   // Invoice uploaded, awaiting verification
    Verified,  // Invoice verified and available for bidding
    Funded,    // Invoice has been funded by an investor
    Paid,      // Invoice has been paid and settled
    Defaulted, // Invoice payment is overdue/defaulted
}

/// Invoice category enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceCategory {
    Services,      // Professional services
    Products,      // Physical products
    Consulting,    // Consulting services
    Manufacturing, // Manufacturing services
    Technology,    // Technology services/products
    Healthcare,    // Healthcare services
    Other,         // Other categories
}

/// Invoice rating structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct InvoiceRating {
    pub rating: u32,       // 1-5 stars
    pub feedback: String,  // Feedback text
    pub rated_by: Address, // Investor who provided the rating
    pub rated_at: u64,     // Timestamp of rating
}

/// Core invoice data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    pub id: BytesN<32>,              // Unique invoice identifier
    pub business: Address,           // Business that uploaded the invoice
    pub amount: i128,                // Total invoice amount
    pub currency: Address,           // Currency token address (XLM = Address::random())
    pub due_date: u64,               // Due date timestamp
    pub status: InvoiceStatus,       // Current status of the invoice
    pub created_at: u64,             // Creation timestamp
    pub description: String,         // Invoice description/metadata
    pub category: InvoiceCategory,   // Invoice category
    pub tags: Vec<String>,           // Invoice tags for better discoverability
    pub funded_amount: i128,         // Amount funded by investors
    pub funded_at: Option<u64>,      // When the invoice was funded
    pub investor: Option<Address>,   // Address of the investor who funded
    pub settled_at: Option<u64>,     // When the invoice was settled
    pub average_rating: Option<u32>, // Average rating (1-5)
    pub total_ratings: u32,          // Total number of ratings
    pub ratings: Vec<InvoiceRating>, // List of all ratings
}

// Use the main error enum from errors.rs
use crate::errors::QuickLendXError;

use crate::audit::{log_invoice_created, log_invoice_status_change, log_invoice_funded};

impl Invoice {
    /// Create a new invoice with audit logging
    pub fn new(
        env: &Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: InvoiceCategory,
        tags: Vec<String>,
    ) -> Self {
        let id = Self::generate_unique_invoice_id(env);
        let created_at = env.ledger().timestamp();

        let invoice = Self {
            id,
            business,
            amount,
            currency,
            due_date,
            status: InvoiceStatus::Pending,
            created_at,
            description,
            category,
            tags,
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: vec![env],
        };
        
        // Log invoice creation
        log_invoice_created(env, &invoice);
        
        invoice
    }

    /// Generate a unique invoice ID
    fn generate_unique_invoice_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        let counter_key = symbol_short!("inv_cnt");
        let counter: u32 = env.storage().instance().get(&counter_key).unwrap_or(0);
        env.storage().instance().set(&counter_key, &(counter + 1));
        
        // Create a unique ID from timestamp, sequence, and counter
        let mut id_bytes = [0u8; 32];
        id_bytes[0..8].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[8..12].copy_from_slice(&sequence.to_be_bytes());
        id_bytes[12..16].copy_from_slice(&counter.to_be_bytes());
        
        BytesN::from_array(env, &id_bytes)
    }

    /// Check if invoice is available for funding
    pub fn is_available_for_funding(&self) -> bool {
        self.status == InvoiceStatus::Verified && self.funded_amount == 0
    }

    /// Check if invoice is overdue
    pub fn is_overdue(&self, current_timestamp: u64) -> bool {
        current_timestamp > self.due_date
    }

    /// Mark invoice as funded with audit logging
    pub fn mark_as_funded(&mut self, env: &Env, investor: Address, funded_amount: i128, timestamp: u64) {
        let old_status = self.status.clone();
        self.status = InvoiceStatus::Funded;
        self.funded_amount = funded_amount;
        self.funded_at = Some(timestamp);
        self.investor = Some(investor.clone());
        
        // Log status change and funding
        log_invoice_status_change(env, self.id.clone(), investor.clone(), old_status, self.status.clone());
        log_invoice_funded(env, self.id.clone(), investor, funded_amount);
    }

    /// Mark invoice as paid with audit logging
    pub fn mark_as_paid(&mut self, env: &Env, actor: Address, timestamp: u64) {
        let old_status = self.status.clone();
        self.status = InvoiceStatus::Paid;
        self.settled_at = Some(timestamp);
        
        // Log status change
        log_invoice_status_change(env, self.id.clone(), actor, old_status, self.status.clone());
    }

    /// Verify the invoice with audit logging
    pub fn verify(&mut self, env: &Env, actor: Address) {
        let old_status = self.status.clone();
        self.status = InvoiceStatus::Verified;
        
        // Log status change
        log_invoice_status_change(env, self.id.clone(), actor, old_status, self.status.clone());
    }

    /// Mark invoice as defaulted
    pub fn mark_as_defaulted(&mut self) {
        self.status = InvoiceStatus::Defaulted;
    }

    /// Check if invoice has ratings
    pub fn has_ratings(&self) -> bool {
        self.total_ratings > 0
    }

    /// Add a rating to the invoice
    pub fn add_rating(&mut self, rating: u32, feedback: String, rated_by: Address, rated_at: u64) -> Result<(), crate::errors::QuickLendXError> {
        if rating < 1 || rating > 5 {
            return Err(crate::errors::QuickLendXError::InvalidRating);
        }

        let new_rating = InvoiceRating {
            rating,
            feedback,
            rated_by,
            rated_at,
        };

        self.ratings.push_back(new_rating);
        self.total_ratings += 1;

        // Recalculate average rating
        let total: u32 = self.ratings.iter().map(|r| r.rating).sum();
        self.average_rating = Some(total / self.total_ratings);

        Ok(())
    }

    /// Get the highest rating
    pub fn get_highest_rating(&self) -> Option<u32> {
        if self.ratings.is_empty() {
            None
        } else {
            Some(self.ratings.iter().map(|r| r.rating).max().unwrap())
        }
    }

    /// Get the lowest rating
    pub fn get_lowest_rating(&self) -> Option<u32> {
        if self.ratings.is_empty() {
            None
        } else {
            Some(self.ratings.iter().map(|r| r.rating).min().unwrap())
        }
    }

    /// Add a tag to the invoice
    pub fn add_tag(&mut self, env: &Env, tag: String) -> Result<(), crate::errors::QuickLendXError> {
        // Validate tag length (1-50 characters)
        if tag.len() < 1 || tag.len() > 50 {
            return Err(crate::errors::QuickLendXError::InvalidTag);
        }

        // Check tag limit (max 10 tags per invoice)
        if self.tags.len() >= 10 {
            return Err(crate::errors::QuickLendXError::TagLimitExceeded);
        }

        // Check if tag already exists
        for existing_tag in self.tags.iter() {
            if existing_tag == tag {
                return Ok(()); // Tag already exists, no need to add
            }
        }

        self.tags.push_back(tag);
        Ok(())
    }

    /// Remove a tag from the invoice
    pub fn remove_tag(&mut self, tag: String) -> Result<(), crate::errors::QuickLendXError> {
        let mut new_tags = Vec::new(&self.tags.env());
        let mut found = false;

        for existing_tag in self.tags.iter() {
            if existing_tag != tag {
                new_tags.push_back(existing_tag.clone());
            } else {
                found = true;
            }
        }

        if !found {
            return Err(crate::errors::QuickLendXError::InvalidTag);
        }

        self.tags = new_tags;
        Ok(())
    }

    /// Check if invoice has a specific tag
    pub fn has_tag(&self, tag: String) -> bool {
        for existing_tag in self.tags.iter() {
            if existing_tag == tag {
                return true;
            }
        }
        false
    }

    /// Update the invoice category
    pub fn update_category(&mut self, category: InvoiceCategory) {
        self.category = category;
    }

    /// Get all tags as a vector
    pub fn get_tags(&self) -> Vec<String> {
        self.tags.clone()
    }
}

/// Storage keys for invoice data
pub struct InvoiceStorage;

impl InvoiceStorage {
    /// Store an invoice
    pub fn store_invoice(env: &Env, invoice: &Invoice) {
        env.storage().instance().set(&invoice.id, invoice);

        // Add to business invoices list
        Self::add_to_business_invoices(env, &invoice.business, &invoice.id);

        // Add to status invoices list
        Self::add_to_status_invoices(env, &invoice.status, &invoice.id);
    }

    /// Get an invoice by ID
    pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        env.storage().instance().get(invoice_id)
    }

    /// Update an invoice
    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        env.storage().instance().set(&invoice.id, invoice);
    }

    /// Get all invoices for a business
    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        let key = (symbol_short!("business"), business.clone());
        env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    /// Get all invoices by status
    pub fn get_invoices_by_status(env: &Env, status: &InvoiceStatus) -> Vec<BytesN<32>> {
        let key = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("default"),
        };
        env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env))
    }

    /// Add invoice to business invoices list
    fn add_to_business_invoices(env: &Env, business: &Address, invoice_id: &BytesN<32>) {
        let key = (symbol_short!("business"), business.clone());
        let mut invoices = Self::get_business_invoices(env, business);
        invoices.push_back(invoice_id.clone());
        env.storage().instance().set(&key, &invoices);
    }

    /// Add invoice to status invoices list
    pub fn add_to_status_invoices(env: &Env, status: &InvoiceStatus, invoice_id: &BytesN<32>) {
        let key = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("default"),
        };
        let mut invoices = env.storage().instance().get(&key).unwrap_or_else(|| Vec::new(env));
        invoices.push_back(invoice_id.clone());
        env.storage().instance().set(&key, &invoices);
    }

    /// Remove invoice from status invoices list
    pub fn remove_from_status_invoices(env: &Env, status: &InvoiceStatus, invoice_id: &BytesN<32>) {
        let key = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("default"),
        };
        let invoices = Self::get_invoices_by_status(env, status);

        // Find and remove the invoice ID
        let mut new_invoices = Vec::new(env);
        for id in invoices.iter() {
            if id != *invoice_id {
                new_invoices.push_back(id);
            }
        }

        env.storage().instance().set(&key, &new_invoices);
    }

    /// Get invoices with ratings above a threshold
    pub fn get_invoices_with_rating_above(env: &Env, threshold: u32) -> Vec<BytesN<32>> {
        let mut high_rated_invoices = vec![env];
        // Get all invoices and filter by rating
        let all_statuses = [InvoiceStatus::Funded, InvoiceStatus::Paid];
        for status in all_statuses.iter() {
            let invoices = Self::get_invoices_by_status(env, status);
            for invoice_id in invoices.iter() {
                if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                    if let Some(avg_rating) = invoice.average_rating {
                        if avg_rating >= threshold {
                            high_rated_invoices.push_back(invoice_id);
                        }
                    }
                }
            }
        }
        high_rated_invoices
    }

    /// Get invoices for a business with ratings above a threshold
    pub fn get_business_invoices_with_rating_above(
        env: &Env,
        business: &Address,
        threshold: u32,
    ) -> Vec<BytesN<32>> {
        let mut high_rated_invoices = vec![env];
        let business_invoices = Self::get_business_invoices(env, business);
        for invoice_id in business_invoices.iter() {
            if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                if let Some(avg_rating) = invoice.average_rating {
                    if avg_rating >= threshold {
                        high_rated_invoices.push_back(invoice_id);
                    }
                }
            }
        }
        high_rated_invoices
    }

    /// Get count of invoices with ratings
    pub fn get_invoices_with_ratings_count(env: &Env) -> u32 {
        let mut count = 0;
        let all_statuses = [InvoiceStatus::Funded, InvoiceStatus::Paid];
        for status in all_statuses.iter() {
            let invoices = Self::get_invoices_by_status(env, status);
            for invoice_id in invoices.iter() {
                if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                    if invoice.has_ratings() {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    /// Get invoices by category
    pub fn get_invoices_by_category(env: &Env, category: &InvoiceCategory) -> Vec<BytesN<32>> {
        let mut category_invoices = vec![env];
        let all_statuses = [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
        ];
        
        for status in all_statuses.iter() {
            let invoices = Self::get_invoices_by_status(env, status);
            for invoice_id in invoices.iter() {
                if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                    if invoice.category == *category {
                        category_invoices.push_back(invoice_id);
                    }
                }
            }
        }
        category_invoices
    }

    /// Get invoices by category and status
    pub fn get_invoices_by_category_and_status(
        env: &Env,
        category: &InvoiceCategory,
        status: &InvoiceStatus,
    ) -> Vec<BytesN<32>> {
        let mut filtered_invoices = vec![env];
        let invoices = Self::get_invoices_by_status(env, status);
        
        for invoice_id in invoices.iter() {
            if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                if invoice.category == *category {
                    filtered_invoices.push_back(invoice_id);
                }
            }
        }
        filtered_invoices
    }

    /// Get invoices by tag
    pub fn get_invoices_by_tag(env: &Env, tag: &String) -> Vec<BytesN<32>> {
        let mut tagged_invoices = vec![env];
        let all_statuses = [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
        ];
        
        for status in all_statuses.iter() {
            let invoices = Self::get_invoices_by_status(env, status);
            for invoice_id in invoices.iter() {
                if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                    if invoice.has_tag(tag.clone()) {
                        tagged_invoices.push_back(invoice_id);
                    }
                }
            }
        }
        tagged_invoices
    }

    /// Get invoices by multiple tags (AND logic - must have all tags)
    pub fn get_invoices_by_tags(env: &Env, tags: &Vec<String>) -> Vec<BytesN<32>> {
        let mut tagged_invoices = vec![env];
        let all_statuses = [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
        ];
        
        for status in all_statuses.iter() {
            let invoices = Self::get_invoices_by_status(env, status);
            for invoice_id in invoices.iter() {
                if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                    let mut has_all_tags = true;
                    for tag in tags.iter() {
                        if !invoice.has_tag(tag.clone()) {
                            has_all_tags = false;
                            break;
                        }
                    }
                    if has_all_tags {
                        tagged_invoices.push_back(invoice_id);
                    }
                }
            }
        }
        tagged_invoices
    }

    /// Get invoice count by category
    pub fn get_invoice_count_by_category(env: &Env, category: &InvoiceCategory) -> u32 {
        Self::get_invoices_by_category(env, category).len() as u32
    }

    /// Get invoice count by tag
    pub fn get_invoice_count_by_tag(env: &Env, tag: &String) -> u32 {
        Self::get_invoices_by_tag(env, tag).len() as u32
    }

    /// Get all available categories
    pub fn get_all_categories(env: &Env) -> Vec<InvoiceCategory> {
        vec![
            env,
            InvoiceCategory::Services,
            InvoiceCategory::Products,
            InvoiceCategory::Consulting,
            InvoiceCategory::Manufacturing,
            InvoiceCategory::Technology,
            InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ]
    }
}