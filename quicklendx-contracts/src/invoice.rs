use core::cmp::{max, min};
use soroban_sdk::{contracttype, symbol_short, vec, Address, BytesN, Env, String, Vec};

use crate::errors::QuickLendXError;
use crate::protocol_limits::{
    check_string_length, MAX_ADDRESS_LENGTH, MAX_DESCRIPTION_LENGTH, MAX_FEEDBACK_LENGTH,
    MAX_NAME_LENGTH, MAX_NOTES_LENGTH, MAX_TAG_LENGTH, MAX_TAX_ID_LENGTH,
    MAX_TRANSACTION_ID_LENGTH,
};

const DEFAULT_INVOICE_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days default grace period

/// Normalize a tag: strip leading/trailing ASCII spaces, then ASCII-lowercase all letters.
///
/// Tags are always stored in their normalized form so that "Tech", " tech ", and "TECH"
/// all collapse to the same canonical key "tech". This ensures consistent duplicate
/// detection and index lookups regardless of the casing or padding the caller supplies.
///
/// # Errors
/// Returns [`QuickLendXError::InvalidTag`] if:
/// - The tag exceeds 50 bytes before normalization (prevents buffer overflow).
/// - The normalized result is empty (e.g. a tag that is all spaces).
/// - The bytes are not valid UTF-8.
pub(crate) fn normalize_tag(env: &Env, tag: &String) -> Result<String, QuickLendXError> {
    let len = tag.len() as usize;
    // Guard against inputs that exceed the maximum tag length.
    if len > 50 {
        return Err(QuickLendXError::InvalidTag);
    }

    let mut buf = [0u8; 50];
    tag.copy_into_slice(&mut buf[..len]);

    // Trim leading ASCII spaces.
    let mut start = 0usize;
    while start < len && buf[start] == b' ' {
        start += 1;
    }
    // Trim trailing ASCII spaces.
    let mut end = len;
    while end > start && buf[end - 1] == b' ' {
        end -= 1;
    }

    if start >= end {
        return Err(QuickLendXError::InvalidTag);
    }

    // ASCII lowercase: shift A-Z (0x41-0x5A) to a-z (0x61-0x7A).
    for b in buf[start..end].iter_mut() {
        if *b >= b'A' && *b <= b'Z' {
            *b += 32;
        }
    }

    let normalized =
        core::str::from_utf8(&buf[start..end]).map_err(|_| QuickLendXError::InvalidTag)?;

    Ok(String::from_str(env, normalized))
}

/// Invoice status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Pending,   // Invoice uploaded, awaiting verification
    Verified,  // Invoice verified and available for bidding
    Funded,    // Invoice has been funded by an investor
    Paid,      // Invoice has been paid and settled
    Defaulted, // Invoice payment is overdue/defaulted
    Cancelled, // Invoice has been cancelled by the business owner
    Refunded,  // Invoice has been refunded (prevents multiple refunds/releases)
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

/// Invoice rating statistics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceRatingStats {
    pub average_rating: u32,
    pub total_ratings: u32,
    pub highest_rating: u32,
    pub lowest_rating: u32,
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

impl InvoiceMetadata {
    pub fn validate(&self) -> Result<(), QuickLendXError> {
        if self.customer_name.len() == 0 || self.customer_name.len() > MAX_NAME_LENGTH {
            return Err(QuickLendXError::InvalidDescription);
        }
        if self.customer_address.len() > MAX_ADDRESS_LENGTH {
            return Err(QuickLendXError::InvalidDescription);
        }
        if self.tax_id.len() > MAX_TAX_ID_LENGTH {
            return Err(QuickLendXError::InvalidDescription);
        }
        if self.line_items.len() > 50 {
            return Err(QuickLendXError::TagLimitExceeded);
        }
        for item in self.line_items.iter() {
            if item.0.len() == 0 || item.0.len() > MAX_DESCRIPTION_LENGTH {
                return Err(QuickLendXError::InvalidDescription);
            }
        }
        if self.notes.len() > MAX_NOTES_LENGTH {
            return Err(QuickLendXError::InvalidDescription);
        }
        Ok(())
    }
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
use crate::audit::{
    log_invoice_created, log_invoice_funded, log_invoice_refunded, log_invoice_status_change,
};

impl Invoice {
    /// Update invoice metadata (business only)
    pub fn update_metadata(
        &mut self,
        _env: &Env,
        business: &Address,
        metadata: InvoiceMetadata,
    ) -> Result<(), QuickLendXError> {
        if self.business != *business {
            return Err(QuickLendXError::Unauthorized);
        }
        business.require_auth();
        metadata.validate()?;
        self.metadata_customer_name = Some(metadata.customer_name.clone());
        self.metadata_customer_address = Some(metadata.customer_address.clone());
        self.metadata_tax_id = Some(metadata.tax_id.clone());
        self.metadata_notes = Some(metadata.notes.clone());
        self.metadata_line_items = metadata.line_items.clone();
        Ok(())
    }

    /// Clear invoice metadata (business only)
    pub fn clear_metadata(&mut self, env: &Env, business: &Address) -> Result<(), QuickLendXError> {
        if self.business != *business {
            return Err(QuickLendXError::Unauthorized);
        }
        business.require_auth();
        self.metadata_customer_name = None;
        self.metadata_customer_address = None;
        self.metadata_tax_id = None;
        self.metadata_notes = None;
        self.metadata_line_items = Vec::new(env);
        Ok(())
    }

    /// Create a new invoice with audit logging.
    ///
    /// All supplied tags are normalized (trimmed, ASCII-lowercased) before storage.
    /// `validate_invoice_tags` must be called by the caller before this function to
    /// ensure the tag list is within limits and free of normalized duplicates.
    pub fn new(
        env: &Env,
        business: Address,
        amount: i128,
        currency: Address,
        due_date: u64,
        description: String,
        category: InvoiceCategory,
        tags: Vec<String>,
    ) -> Result<Self, QuickLendXError> {
        check_string_length(&description, MAX_DESCRIPTION_LENGTH)?;

        // Normalize every tag before storage so the on-chain representation is always
        // in canonical form regardless of how the caller formatted the input.
        let mut normalized_tags = Vec::new(env);
        for tag in tags.iter() {
            normalized_tags.push_back(normalize_tag(env, &tag)?);
        }

        let id = Self::generate_unique_invoice_id(env)?;
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
            tags: normalized_tags,
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

        Ok(invoice)
    }

    /// @notice Derives a deterministic invoice ID candidate from a ledger slot and counter.
    /// @dev The candidate format is `timestamp || sequence || counter || 16 zero bytes`.
    /// @param timestamp Current ledger timestamp used for allocation.
    /// @param sequence Current ledger sequence used for allocation.
    /// @param counter Monotonic invoice counter for the contract instance.
    /// @return A deterministic `BytesN<32>` candidate that can be checked for collisions.
    pub(crate) fn derive_invoice_id(
        env: &Env,
        timestamp: u64,
        sequence: u32,
        counter: u32,
    ) -> BytesN<32> {
        let mut id_bytes = [0u8; 32];
        id_bytes[0..8].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[8..12].copy_from_slice(&sequence.to_be_bytes());
        id_bytes[12..16].copy_from_slice(&counter.to_be_bytes());
        BytesN::from_array(env, &id_bytes)
    }

    /// @notice Allocates a unique deterministic invoice ID for the current ledger slot.
    /// @dev Probes forward on the monotonic counter until it finds an unused invoice key in
    /// instance storage, so an existing invoice cannot be silently overwritten if a candidate
    /// collides. Counter overflow aborts with `StorageError`.
    /// @return A storage-safe invoice ID for the new invoice.
    fn generate_unique_invoice_id(env: &Env) -> Result<BytesN<32>, QuickLendXError> {
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        let counter_key = symbol_short!("inv_cnt");
        let mut counter: u32 = env.storage().instance().get(&counter_key).unwrap_or(0);

        loop {
            let candidate = Self::derive_invoice_id(env, timestamp, sequence, counter);
            if InvoiceStorage::get_invoice(env, &candidate).is_none() {
                let next_counter = counter
                    .checked_add(1)
                    .ok_or(QuickLendXError::StorageError)?;
                env.storage().instance().set(&counter_key, &next_counter);
                return Ok(candidate);
            }

            counter = counter
                .checked_add(1)
                .ok_or(QuickLendXError::StorageError)?;
        }
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

    /// Mark invoice as refunded with audit logging
    pub fn mark_as_refunded(&mut self, env: &Env, actor: Address) {
        let old_status = self.status.clone();
        self.status = InvoiceStatus::Refunded;

        // Log status change
        log_invoice_status_change(
            env,
            self.id.clone(),
            actor.clone(),
            old_status,
            self.status.clone(),
        );
        log_invoice_refunded(env, self.id.clone(), actor);
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

        check_string_length(&transaction_id, MAX_TRANSACTION_ID_LENGTH)?;

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
        let denominator = max(self.amount, 1i128);
        let percentage = capped_total
            .saturating_mul(100i128)
            .checked_div(denominator)
            .unwrap_or(0);
        min(percentage, 100i128) as u32
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
    pub fn set_metadata(
        &mut self,
        env: &Env,
        metadata: Option<InvoiceMetadata>,
    ) -> Result<(), QuickLendXError> {
        match metadata {
            Some(data) => {
                check_string_length(&data.customer_name, MAX_NAME_LENGTH)?;
                check_string_length(&data.customer_address, MAX_ADDRESS_LENGTH)?;
                check_string_length(&data.tax_id, MAX_TAX_ID_LENGTH)?;
                check_string_length(&data.notes, MAX_NOTES_LENGTH)?;

                for item in data.line_items.iter() {
                    check_string_length(&item.0, MAX_DESCRIPTION_LENGTH)?;
                }

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
        Ok(())
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

    /// Cancel the invoice (only if Pending or Verified, not Funded)
    pub fn cancel(&mut self, env: &Env, actor: Address) -> Result<(), QuickLendXError> {
        // Can only cancel if Pending or Verified (not yet funded)
        if self.status != InvoiceStatus::Pending && self.status != InvoiceStatus::Verified {
            return Err(QuickLendXError::InvalidStatus);
        }

        let old_status = self.status.clone();
        self.status = InvoiceStatus::Cancelled;

        // Log status change
        log_invoice_status_change(env, self.id.clone(), actor, old_status, self.status.clone());

        Ok(())
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

        check_string_length(&feedback, MAX_FEEDBACK_LENGTH)?;

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
        self.total_ratings = self.total_ratings.saturating_add(1);

        // Calculate new average rating (overflow-safe: sum is u64, count is u32)
        let sum: u64 = self.ratings.iter().map(|r| r.rating as u64).sum();
        let count = self.total_ratings as u64;
        let avg = if count > 0 {
            (sum / count).min(5) as u32
        } else {
            0
        };
        self.average_rating = Some(avg);

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

    /// Get comprehensive rating statistics for this invoice
    pub fn get_invoice_rating_stats(&self) -> InvoiceRatingStats {
        InvoiceRatingStats {
            average_rating: self.average_rating.unwrap_or(0),
            total_ratings: self.total_ratings,
            highest_rating: self.get_highest_rating().unwrap_or(0),
            lowest_rating: self.get_lowest_rating().unwrap_or(0),
        }
    }

    /// Add a tag to the invoice.
    ///
    /// The tag is normalized (trimmed, ASCII-lowercased) before storage so that
    /// "Tech" and " tech " both resolve to "tech". Duplicate detection uses the
    /// normalized form: adding an already-present normalized tag is a no-op.
    pub fn add_tag(
        &mut self,
        env: &Env,
        tag: String,
    ) -> Result<(), crate::errors::QuickLendXError> {
        // 🔒 AUTH PROTECTION: Only the business that created the invoice can add tags.
        self.business.require_auth();

        let normalized = normalize_tag(env, &tag)?;

        if normalized.len() < 1 || normalized.len() > 50 {
            return Err(crate::errors::QuickLendXError::InvalidTag);
        }

        if self.tags.len() >= 10 {
            return Err(crate::errors::QuickLendXError::TagLimitExceeded);
        }

        for existing_tag in self.tags.iter() {
            if existing_tag == normalized {
                return Ok(());
            }
        }

        self.tags.push_back(normalized.clone());
        
        // Update Index for discoverability
        InvoiceStorage::add_tag_index(env, &normalized, &self.id);
        
        Ok(())
    }

    /// Remove a tag from the invoice (Business Owner Only).
    pub fn remove_tag(&mut self, tag: String) -> Result<(), crate::errors::QuickLendXError> {
        // 🔒 AUTH PROTECTION
        self.business.require_auth();

        let env = self.tags.env();
        let normalized = normalize_tag(&env, &tag)?;
        let mut new_tags = Vec::new(&env);
        let mut found = false;

        for existing_tag in self.tags.iter() {
            if existing_tag != normalized {
                new_tags.push_back(existing_tag.clone());
            } else {
                found = true;
            }
        }

        if !found {
            return Err(crate::errors::QuickLendXError::InvalidTag);
        }

        // Remove from Index first before modifying self.tags
        InvoiceStorage::remove_tag_index(&env, &normalized, &self.id);
        
        // Now assign the new tags
        self.tags = new_tags;
        
        Ok(())
    }

    /// Check if invoice has a specific tag.
    ///
    /// The query tag is normalized before comparison, so `has_tag("Tech")` returns
    /// `true` when the stored tag is "tech". Returns `false` for any input that
    /// normalizes to an empty string.
    pub fn has_tag(&self, tag: String) -> bool {
        let env = self.tags.env();
        let Ok(normalized) = normalize_tag(&env, &tag) else {
            return false;
        };
        for existing_tag in self.tags.iter() {
            if existing_tag == normalized {
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

pub(crate) const TOTAL_INVOICE_COUNT_KEY: soroban_sdk::Symbol = symbol_short!("total_iv");

/// Storage keys for invoice data
pub struct InvoiceStorage;

impl InvoiceStorage {
    fn category_key(category: &InvoiceCategory) -> (soroban_sdk::Symbol, InvoiceCategory) {
        (symbol_short!("cat_idx"), category.clone())
    }

    fn tag_key(tag: &String) -> (soroban_sdk::Symbol, String) {
        (symbol_short!("tag_idx"), tag.clone())
    }

    /// @notice Adds an invoice to the category index.
    /// @dev Deduplication guard: the invoice ID is appended only if not already
    ///      present, preventing duplicate entries that would corrupt count queries.
    /// @param env   The contract environment.
    /// @param category   The category bucket to update.
    /// @param invoice_id The invoice to register.
    /// @security Caller must ensure `invoice_id` refers to a stored invoice with
    ///           the matching category field to keep the index consistent.
    pub fn add_category_index(env: &Env, category: &InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Self::category_key(category);
        let mut invoices = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));

        let mut found = false;
        for existing in invoices.iter() {
            if existing == *invoice_id {
                found = true;
                break;
            }
        }
        if !found {
            invoices.push_back(invoice_id.clone());
            env.storage().instance().set(&key, &invoices);
        }
    }

    /// @notice Removes an invoice from the category index.
    /// @dev Rebuilds the bucket without the target ID. Safe to call even if the
    ///      ID is absent (no-op). Must be called with the invoice's *old* category
    ///      before calling `add_category_index` with the new one to avoid stale entries.
    /// @param env   The contract environment.
    /// @param category   The category bucket to update.
    /// @param invoice_id The invoice to deregister.
    pub fn remove_category_index(env: &Env, category: &InvoiceCategory, invoice_id: &BytesN<32>) {
        let key = Self::category_key(category);
        if let Some(invoices) = env.storage().instance().get::<_, Vec<BytesN<32>>>(&key) {
            let mut new_invoices = Vec::new(env);
            for id in invoices.iter() {
                if id != *invoice_id {
                    new_invoices.push_back(id);
                }
            }
            env.storage().instance().set(&key, &new_invoices);
        }
    }

    pub fn add_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Self::tag_key(tag);
        let mut invoices = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        let mut found = false;
        for existing in invoices.iter() {
            if existing == *invoice_id {
                found = true;
                break;
            }
        }
        if !found {
            invoices.push_back(invoice_id.clone());
            env.storage().instance().set(&key, &invoices);
        }
    }

    pub fn remove_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>) {
        let key = Self::tag_key(tag);
        if let Some(invoices) = env.storage().instance().get::<_, Vec<BytesN<32>>>(&key) {
            let mut new_invoices = Vec::new(env);
            for id in invoices.iter() {
                if id != *invoice_id {
                    new_invoices.push_back(id);
                }
            }
            env.storage().instance().set(&key, &new_invoices);
        }
    }

    /// Store an invoice
    pub fn store_invoice(env: &Env, invoice: &Invoice) {
        let is_new = !env.storage().instance().has(&invoice.id);
        env.storage().instance().set(&invoice.id, invoice);

        // Update total count if this is a new invoice
        if is_new {
            let mut count: u32 = env
                .storage()
                .instance()
                .get(&TOTAL_INVOICE_COUNT_KEY)
                .unwrap_or(0);
            count = count.saturating_add(1);
            env.storage()
                .instance()
                .set(&TOTAL_INVOICE_COUNT_KEY, &count);
        }

        // Add to business invoices list
        Self::add_to_business_invoices(env, &invoice.business, &invoice.id);

        // Add to status invoices list
        Self::add_to_status_invoices(env, &invoice.status, &invoice.id);

        // Add to category index
        Self::add_category_index(env, &invoice.category, &invoice.id);

        // Add to tag indexes
        for tag in invoice.tags.iter() {
            Self::add_tag_index(env, &tag, &invoice.id);
        }
    }

    /// Get an invoice by ID
    pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice> {
        env.storage().instance().get(invoice_id)
    }

    /// Update an invoice
    pub fn update_invoice(env: &Env, invoice: &Invoice) {
        env.storage().instance().set(&invoice.id, invoice);
    }

    /// Clear all invoices from storage (used by backup restore)
    pub fn clear_all(env: &Env) {
        // Clear each invoice from each status list
        for status in [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
            InvoiceStatus::Cancelled,
        ] {
            let ids = Self::get_invoices_by_status(env, &status);
            for id in ids.iter() {
                env.storage().instance().remove(&id);
            }
            let key = match status {
                InvoiceStatus::Pending => symbol_short!("pending"),
                InvoiceStatus::Verified => symbol_short!("verified"),
                InvoiceStatus::Funded => symbol_short!("funded"),
                InvoiceStatus::Paid => symbol_short!("paid"),
                InvoiceStatus::Defaulted => symbol_short!("default"),
                InvoiceStatus::Cancelled => symbol_short!("cancel"),
                InvoiceStatus::Refunded => symbol_short!("refunded"),
            };
            env.storage().instance().remove(&key);
        }

        // Unify with other storage cleanups
        crate::storage::StorageManager::clear_all_mappings(env);
    }

    /// Get all invoices for a business
    pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>> {
        let key = (symbol_short!("business"), business.clone());
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Count active invoices for a business (excludes Cancelled and Paid invoices)
    pub fn count_active_business_invoices(env: &Env, business: &Address) -> u32 {
        let business_invoices = Self::get_business_invoices(env, business);
        let mut count = 0u32;
        for invoice_id in business_invoices.iter() {
            if let Some(invoice) = Self::get_invoice(env, &invoice_id) {
                // Only count active invoices (not Cancelled or Paid)
                if !matches!(
                    invoice.status,
                    InvoiceStatus::Cancelled | InvoiceStatus::Paid
                ) {
                    count = count.saturating_add(1);
                }
            }
        }
        count
    }

    /// Get all invoices by status
    pub fn get_invoices_by_status(env: &Env, status: &InvoiceStatus) -> Vec<BytesN<32>> {
        let key = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("default"),
            InvoiceStatus::Cancelled => symbol_short!("canceld"),
            InvoiceStatus::Refunded => symbol_short!("refundd"),
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
            InvoiceStatus::Cancelled => symbol_short!("canceld"),
            InvoiceStatus::Refunded => symbol_short!("refundd"),
        };
        let mut invoices = env
            .storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env));
        if !invoices.iter().any(|id| id == *invoice_id) {
            invoices.push_back(invoice_id.clone());
            env.storage().instance().set(&key, &invoices);
        }
    }

    /// Remove invoice from status invoices list
    pub fn remove_from_status_invoices(env: &Env, status: &InvoiceStatus, invoice_id: &BytesN<32>) {
        let key = match status {
            InvoiceStatus::Pending => symbol_short!("pending"),
            InvoiceStatus::Verified => symbol_short!("verified"),
            InvoiceStatus::Funded => symbol_short!("funded"),
            InvoiceStatus::Paid => symbol_short!("paid"),
            InvoiceStatus::Defaulted => symbol_short!("default"),
            InvoiceStatus::Cancelled => symbol_short!("canceld"),
            InvoiceStatus::Refunded => symbol_short!("refundd"),
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

        // 🛡️ INDEX ROLLBACK PROTECTION
        // Remove the invoice from the old category index before updating
        InvoiceStorage::remove_category_index(env, &self.category, &self.id);

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

    /// Completely remove an invoice from storage and all its indexes (used by backup restore)
    pub fn delete_invoice(env: &Env, invoice_id: &BytesN<32>) {
        if let Some(invoice) = Self::get_invoice(env, invoice_id) {
            // Remove from status index
            Self::remove_from_status_invoices(env, &invoice.status, invoice_id);

            // Remove from business index
            let business_key = (symbol_short!("business"), invoice.business.clone());
            if let Some(invoices) = env
                .storage()
                .instance()
                .get::<_, Vec<BytesN<32>>>(&business_key)
            {
                let mut new_invoices = Vec::new(env);
                for id in invoices.iter() {
                    if id != *invoice_id {
                        new_invoices.push_back(id);
                    }
                }
                env.storage().instance().set(&business_key, &new_invoices);
            }

            // Remove from category index
            Self::remove_category_index(env, &invoice.category, invoice_id);

            // Remove from tag indexes
            for tag in invoice.tags.iter() {
                Self::remove_tag_index(env, &tag, invoice_id);
            }

            // Remove metadata indexes if present
            if let Some(md) = invoice.metadata() {
                Self::remove_metadata_indexes(env, &md, invoice_id);
            }

            // Decrement total count
            let mut count: u32 = env
                .storage()
                .instance()
                .get(&TOTAL_INVOICE_COUNT_KEY)
                .unwrap_or(0);
            if count > 0 {
                count -= 1;
                env.storage()
                    .instance()
                    .set(&TOTAL_INVOICE_COUNT_KEY, &count);
            }
        }

        // Add to the new category index
        InvoiceStorage::add_category_index(env, &self.category, &self.id);
    }

    /// Get total count of active invoices in the system
    pub fn get_total_invoice_count(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&TOTAL_INVOICE_COUNT_KEY)
            .unwrap_or(0)
    }
}
