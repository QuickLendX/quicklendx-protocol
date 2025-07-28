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

impl Invoice {
    /// Create a new invoice
    pub fn new(
        env: &Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
    ) -> Self {
        let id = Self::generate_unique_invoice_id(env);
        let created_at = env.ledger().timestamp();

        Self {
            id,
            business,
            amount,
            currency,
            due_date,
            status: InvoiceStatus::Pending,
            created_at,
            description,
            funded_amount: 0,
            funded_at: None,
            investor: None,
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: vec![env],
        }
    }

    /// Check if invoice is available for funding
    pub fn is_available_for_funding(&self) -> bool {
        self.status == InvoiceStatus::Verified && self.funded_amount == 0
    }

    /// Check if invoice is overdue
    pub fn is_overdue(&self, current_timestamp: u64) -> bool {
        current_timestamp > self.due_date
    }

    /// Mark invoice as funded
    pub fn mark_as_funded(&mut self, investor: Address, funded_amount: i128, timestamp: u64) {
        self.status = InvoiceStatus::Funded;
        self.funded_amount = funded_amount;
        self.funded_at = Some(timestamp);
        self.investor = Some(investor);
    }

    /// Mark invoice as paid
    pub fn mark_as_paid(&mut self, timestamp: u64) {
        self.status = InvoiceStatus::Paid;
        self.settled_at = Some(timestamp);
    }

    /// Mark invoice as defaulted
    pub fn mark_as_defaulted(&mut self) {
        self.status = InvoiceStatus::Defaulted;
    }

    /// Verify the invoice
    pub fn verify(&mut self) {
        self.status = InvoiceStatus::Verified;
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

    /// Check if invoice has any ratings
    pub fn has_ratings(&self) -> bool {
        self.total_ratings > 0
    }

    /// Get the highest rating received
    pub fn get_highest_rating(&self) -> Option<u32> {
        if self.ratings.is_empty() {
            return None;
        }
        Some(self.ratings.iter().map(|r| r.rating).max().unwrap())
    }

    /// Get the lowest rating received
    pub fn get_lowest_rating(&self) -> Option<u32> {
        if self.ratings.is_empty() {
            return None;
        }
        Some(self.ratings.iter().map(|r| r.rating).min().unwrap())
    }

    /// Generate a unique invoice ID
    fn generate_unique_invoice_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("inv_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        env.storage().instance().set(&counter_key, &(counter + 1));

        let mut id_bytes = [0u8; 32];
        // Add invoice prefix to distinguish from other entity types
        id_bytes[0] = 0x1A; // 'I' for Invoice (hex)
        id_bytes[1] = 0x4E; // 'N' for iNvoice (hex)
                            // Embed timestamp in next 8 bytes
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        id_bytes[10..18].copy_from_slice(&counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness
        for i in 18..32 {
            id_bytes[i] = ((timestamp + counter + 0x1A4E) % 256) as u8;
        }
        BytesN::from_array(env, &id_bytes)
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
}
