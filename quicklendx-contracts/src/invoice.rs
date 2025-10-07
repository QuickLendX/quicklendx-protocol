use core::cmp::{max, min};
use soroban_sdk::{contracttype, symbol_short, vec, Address, BytesN, Env, String, Vec};

use crate::errors::QuickLendXError;

const DEFAULT_INVOICE_GRACE_PERIOD: u64 = 14 * 24 * 60 * 60;

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

/// Dispute status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeStatus {
    None,        // No dispute exists
    Disputed,    // Dispute has been created
    UnderReview, // Dispute is under review
    Resolved,    // Dispute has been resolved
}

/// Dispute structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub created_by: Address,  // Address of the party who created the dispute
    pub created_at: u64,      // Timestamp when dispute was created
    pub reason: String,       // Reason for the dispute
    pub evidence: String,     // Evidence provided by the disputing party
    pub resolution: String,   // Resolution description (empty if not resolved)
    pub resolved_by: Address, // Address of the party who resolved the dispute (zero address if not resolved)
    pub resolved_at: u64,     // Timestamp when dispute was resolved (0 if not resolved)
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

/// Compact representation of a line item stored on-chain
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineItemRecord(pub String, pub i128, pub i128, pub i128);

/// Metadata associated with an invoice
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvoiceMetadata {
    pub customer_name: String,
    pub customer_address: String,
    pub tax_id: String,
    pub line_items: Vec<LineItemRecord>,
    pub notes: String,
}

/// Individual payment record for an invoice
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecord {
    pub amount: i128,           // Amount paid in this transaction
    pub timestamp: u64,         // When the payment was recorded
    pub transaction_id: String, // External transaction reference
}

/// Core invoice data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    pub id: BytesN<32>,        // Unique invoice identifier
    pub business: Address,     // Business that uploaded the invoice
    pub amount: i128,          // Total invoice amount
    pub currency: Address,     // Currency token address (XLM = Address::random())
    pub due_date: u64,         // Due date timestamp
    pub status: InvoiceStatus, // Current status of the invoice
    pub created_at: u64,       // Creation timestamp
    pub description: String,   // Invoice description/metadata
    pub metadata_customer_name: Option<String>,
    pub metadata_customer_address: Option<String>,
    pub metadata_tax_id: Option<String>,
    pub metadata_notes: Option<String>,
    pub metadata_line_items: Vec<LineItemRecord>,
    pub category: InvoiceCategory,           // Invoice category
    pub tags: Vec<String>,                   // Invoice tags for better discoverability
    pub funded_amount: i128,                 // Amount funded by investors
    pub funded_at: Option<u64>,              // When the invoice was funded
    pub investor: Option<Address>,           // Address of the investor who funded
    pub settled_at: Option<u64>,             // When the invoice was settled
    pub average_rating: Option<u32>,         // Average rating (1-5)
    pub total_ratings: u32,                  // Total number of ratings
    pub ratings: Vec<InvoiceRating>,         // List of all ratings
    pub dispute_status: DisputeStatus,       // Current dispute status
    pub dispute: Dispute,                    // Dispute details if any
    pub total_paid: i128,                    // Aggregate amount paid towards the invoice
    pub payment_history: Vec<PaymentRecord>, // History of partial payments
}

// Use the main error enum from errors.rs
use crate::audit::{log_invoice_created, log_invoice_funded, log_invoice_status_change};

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
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: Vec::new(env),
            category,
            tags,
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: vec![env],
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: Address::from_str(
                    env,
                    "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
                ),
                created_at: 0,
                reason: String::from_str(env, ""),
                evidence: String::from_str(env, ""),
                resolution: String::from_str(env, ""),
                resolved_by: Address::from_str(
                    env,
                    "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
                ),
                resolved_at: 0,
            },
            total_paid: 0,
            payment_history: vec![env],
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

    pub const DEFAULT_GRACE_PERIOD: u64 = DEFAULT_INVOICE_GRACE_PERIOD;

    /// Check if invoice is overdue
    pub fn is_overdue(&self, current_timestamp: u64) -> bool {
        current_timestamp > self.due_date
    }

    /// Calculate the timestamp when the grace period ends
    pub fn grace_deadline(&self, grace_period: u64) -> u64 {
        self.due_date.saturating_add(grace_period)
    }

    /// Check if the invoice should be defaulted and handle it if necessary
    pub fn check_and_handle_expiration(
        &self,
        env: &Env,
        grace_period: u64,
    ) -> Result<bool, QuickLendXError> {
        if self.status != InvoiceStatus::Funded {
            return Ok(false);
        }

        let now = env.ledger().timestamp();
        if now <= self.grace_deadline(grace_period) {
            return Ok(false);
        }

        crate::defaults::handle_default(env, &self.id)?;
        Ok(true)
    }

    /// Mark invoice as funded with audit logging
    pub fn mark_as_funded(
        &mut self,
        env: &Env,
        investor: Address,
        funded_amount: i128,
        timestamp: u64,
    ) {
        let old_status = self.status.clone();
        self.status = InvoiceStatus::Funded;
        self.funded_amount = funded_amount;
        self.funded_at = Some(timestamp);
        self.investor = Some(investor.clone());

        // Log status change and funding
        log_invoice_status_change(
            env,
            self.id.clone(),
            investor.clone(),
            old_status,
            self.status.clone(),
        );
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

    /// Add a payment record and update totals
    pub fn record_payment(
        &mut self,
        env: &Env,
        amount: i128,
        transaction_id: String,
    ) -> Result<u32, QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let record = PaymentRecord {
            amount,
            timestamp: env.ledger().timestamp(),
            transaction_id,
        };
        self.payment_history.push_back(record);
        self.total_paid = self.total_paid.saturating_add(amount);

        Ok(self.payment_progress())
    }

    /// Calculate the payment progress percentage (0-100)
    pub fn payment_progress(&self) -> u32 {
        if self.amount <= 0 {
            return 0;
        }

        let capped_total = max(self.total_paid, 0i128);
        let mut percentage = (capped_total.saturating_mul(100i128)) / max(self.amount, 1i128);
        percentage = min(percentage, 100i128);
        percentage as u32
    }

    /// Check if the invoice has been fully paid
    pub fn is_fully_paid(&self) -> bool {
        self.total_paid >= self.amount
    }

    /// Retrieve metadata if present
    pub fn metadata(&self) -> Option<InvoiceMetadata> {
        let name = self.metadata_customer_name.clone()?;
        let address = self.metadata_customer_address.clone()?;
        let tax = self.metadata_tax_id.clone()?;
        let notes = self.metadata_notes.clone()?;

        Some(InvoiceMetadata {
            customer_name: name,
            customer_address: address,
            tax_id: tax,
            line_items: self.metadata_line_items.clone(),
            notes,
        })
    }

    /// Update structured metadata attached to the invoice
    pub fn set_metadata(&mut self, env: &Env, metadata: Option<InvoiceMetadata>) {
        match metadata {
            Some(data) => {
                self.metadata_customer_name = Some(data.customer_name);
                self.metadata_customer_address = Some(data.customer_address);
                self.metadata_tax_id = Some(data.tax_id);
                self.metadata_notes = Some(data.notes);
                self.metadata_line_items = data.line_items;
            }
            None => {
                self.metadata_customer_name = None;
                self.metadata_customer_address = None;
                self.metadata_tax_id = None;
                self.metadata_notes = None;
                self.metadata_line_items = Vec::new(env);
            }
        }
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

    /// Add a rating to the invoice
    pub fn add_rating(
        &mut self,
        rating: u32,
        feedback: String,
        rater: Address,
        timestamp: u64,
    ) -> Result<(), QuickLendXError> {
        // Validate invoice is funded
        if self.status != InvoiceStatus::Funded && self.status != InvoiceStatus::Paid {
            return Err(QuickLendXError::NotFunded);
        }

        // Verify rater is the investor
        if self.investor.as_ref() != Some(&rater) {
            return Err(QuickLendXError::NotRater);
        }

        // Validate rating value
        if rating < 1 || rating > 5 {
            return Err(QuickLendXError::InvalidRating);
        }

        // Check if rater has already rated
        for existing_rating in self.ratings.iter() {
            if existing_rating.rated_by == rater {
                return Err(QuickLendXError::AlreadyRated);
            }
        }

        // Create new rating
        let invoice_rating = InvoiceRating {
            rating,
            feedback,
            rated_by: rater,
            rated_at: timestamp,
        };

        // Add rating
        self.ratings.push_back(invoice_rating);
        self.total_ratings += 1;

        // Calculate new average rating
        let sum: u64 = self.ratings.iter().map(|r| r.rating as u64).sum();
        self.average_rating = Some((sum / self.total_ratings as u64) as u32);

        Ok(())
    }

    /// Get ratings above a threshold
    pub fn get_ratings_above(&self, env: &Env, threshold: u32) -> Vec<InvoiceRating> {
        let mut filtered = vec![env];
        for rating in self.ratings.iter() {
            if rating.rating >= threshold {
                filtered.push_back(rating);
            }
        }
        filtered
    }

    /// Get all ratings for the invoice
    pub fn get_all_ratings(&self) -> &Vec<InvoiceRating> {
        &self.ratings
    }

    /// Check if invoice has ratings
    pub fn has_ratings(&self) -> bool {
        self.total_ratings > 0
    }

    /// Get the highest rating received
    pub fn get_highest_rating(&self) -> Option<u32> {
        if self.ratings.is_empty() {
            None
        } else {
            Some(self.ratings.iter().map(|r| r.rating).max().unwrap())
        }
    }

    /// Get the lowest rating received
    pub fn get_lowest_rating(&self) -> Option<u32> {
        if self.ratings.is_empty() {
            None
        } else {
            Some(self.ratings.iter().map(|r| r.rating).min().unwrap())
        }
    }

    /// Add a tag to the invoice
    pub fn add_tag(
        &mut self,
        _env: &Env,
        tag: String,
    ) -> Result<(), crate::errors::QuickLendXError> {
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
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
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
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
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
        let mut invoices = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
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

    fn metadata_customer_key(customer: &String) -> (soroban_sdk::Symbol, String) {
        (symbol_short!("meta_c"), customer.clone())
    }

    fn metadata_tax_key(tax_id: &String) -> (soroban_sdk::Symbol, String) {
        (symbol_short!("meta_t"), tax_id.clone())
    }

    fn add_to_metadata_index(
        env: &Env,
        key: &(soroban_sdk::Symbol, String),
        invoice_id: &BytesN<32>,
    ) {
        let mut invoices = env
            .storage()
            .instance()
            .get(key)
            .unwrap_or_else(|| Vec::new(env));
        for existing in invoices.iter() {
            if existing == *invoice_id {
                return;
            }
        }
        invoices.push_back(invoice_id.clone());
        env.storage().instance().set(key, &invoices);
    }

    fn remove_from_metadata_index(
        env: &Env,
        key: &(soroban_sdk::Symbol, String),
        invoice_id: &BytesN<32>,
    ) {
        let existing: Option<Vec<BytesN<32>>> = env.storage().instance().get(key);
        if let Some(invoices) = existing {
            let mut filtered = Vec::new(env);
            for id in invoices.iter() {
                if id != *invoice_id {
                    filtered.push_back(id);
                }
            }
            env.storage().instance().set(key, &filtered);
        }
    }

    pub fn add_metadata_indexes(env: &Env, invoice: &Invoice) {
        if let Some(name) = &invoice.metadata_customer_name {
            if name.len() > 0 {
                let key = Self::metadata_customer_key(name);
                Self::add_to_metadata_index(env, &key, &invoice.id);
            }
        }

        if let Some(tax) = &invoice.metadata_tax_id {
            if tax.len() > 0 {
                let key = Self::metadata_tax_key(tax);
                Self::add_to_metadata_index(env, &key, &invoice.id);
            }
        }
    }

    pub fn remove_metadata_indexes(env: &Env, metadata: &InvoiceMetadata, invoice_id: &BytesN<32>) {
        if metadata.customer_name.len() > 0 {
            let key = Self::metadata_customer_key(&metadata.customer_name);
            Self::remove_from_metadata_index(env, &key, invoice_id);
        }

        if metadata.tax_id.len() > 0 {
            let key = Self::metadata_tax_key(&metadata.tax_id);
            Self::remove_from_metadata_index(env, &key, invoice_id);
        }
    }

    pub fn get_invoices_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&Self::metadata_customer_key(customer_name))
            .unwrap_or_else(|| Vec::new(env))
    }

    pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>> {
        env.storage()
            .instance()
            .get(&Self::metadata_tax_key(tax_id))
            .unwrap_or_else(|| Vec::new(env))
    }
}
