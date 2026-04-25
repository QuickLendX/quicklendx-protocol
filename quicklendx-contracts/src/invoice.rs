use crate::errors::QuickLendXError;
use crate::protocol_limits::{check_string_length, MAX_FEEDBACK_LENGTH};
use crate::storage::DataKey;
use crate::verification::normalize_tag;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

pub use crate::storage::InvoiceStorage;
pub use crate::types::{
    Dispute, DisputeStatus, Invoice, InvoiceCategory, InvoiceMetadata, InvoiceRating,
    InvoiceStatus, LineItemRecord, PaymentRecord,
};

/// Maximum normalized tags allowed per invoice.
///
/// Limiting tag cardinality prevents unbounded metadata growth and keeps
/// tag-based query/index operations predictable.
pub const MAX_INVOICE_TAGS: u32 = 10;

/// Maximum ratings retained per invoice.
///
/// Bounding this vector prevents unbounded on-chain storage growth from
/// repeated rating submissions over time.
pub const MAX_RATINGS_PER_INVOICE: u32 = 100;

impl Invoice {
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
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let mut normalized_tags = Vec::new(env);
        for tag in tags.iter() {
            let normalized = normalize_tag(env, &tag)?;
            let mut exists = false;
            for existing in normalized_tags.iter() {
                if existing == normalized {
                    exists = true;
                    break;
                }
            }
            if !exists {
                if normalized_tags.len() >= MAX_INVOICE_TAGS {
                    return Err(QuickLendXError::TagLimitExceeded);
                }
                normalized_tags.push_back(normalized);
            }
        }

        Ok(Self {
            id: Self::allocate_id(env),
            business,
            amount,
            currency,
            due_date,
            status: InvoiceStatus::Pending,
            created_at: env.ledger().timestamp(),
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
            ratings: Vec::new(env),
            dispute_status: DisputeStatus::None,
            dispute: Self::empty_dispute(env),
            total_paid: 0,
            payment_history: Vec::new(env),
        })
    }

    fn allocate_id(env: &Env) -> BytesN<32> {
        loop {
            let counter = InvoiceStorage::next_count(env) as u32 - 1;
            let mut bytes = [0u8; 32];
            bytes[0..8].copy_from_slice(&env.ledger().timestamp().to_be_bytes());
            bytes[8..12].copy_from_slice(&env.ledger().sequence().to_be_bytes());
            bytes[12..16].copy_from_slice(&counter.to_be_bytes());
            let invoice_id = BytesN::from_array(env, &bytes);
            if !env
                .storage()
                .persistent()
                .has(&DataKey::Invoice(invoice_id.clone()))
            {
                return invoice_id;
            }
        }
    }

    fn empty_dispute(env: &Env) -> Dispute {
        Dispute {
            created_by: zero_address(env),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            resolution: String::from_str(env, ""),
            resolved_by: zero_address(env),
            resolved_at: 0,
        }
    }

    pub fn is_available_for_funding(&self) -> bool {
        self.status == InvoiceStatus::Verified
            && self.funded_amount == 0
            && self.funded_at.is_none()
            && self.investor.is_none()
    }

    pub fn verify(&mut self, _env: &Env, _actor: Address) {
        self.status = InvoiceStatus::Verified;
    }

    pub fn mark_as_funded(&mut self, _env: &Env, investor: Address, amount: i128, timestamp: u64) {
        self.status = InvoiceStatus::Funded;
        self.funded_amount = amount;
        self.funded_at = Some(timestamp);
        self.investor = Some(investor);
    }

    pub fn mark_as_paid(&mut self, _env: &Env, _actor: Address, timestamp: u64) {
        self.status = InvoiceStatus::Paid;
        self.total_paid = self.amount;
        self.settled_at = Some(timestamp);
    }

    pub fn mark_as_defaulted(&mut self) {
        self.status = InvoiceStatus::Defaulted;
    }

    pub fn mark_as_refunded(&mut self, env: &Env, _actor: Address) {
        self.status = InvoiceStatus::Refunded;
        self.funded_amount = 0;
        self.funded_at = None;
        self.investor = None;
        self.total_paid = 0;
        self.payment_history = Vec::new(env);
    }

    pub fn get_tags(&self) -> Vec<String> {
        self.tags.clone()
    }

    pub fn has_tag(&self, tag: String) -> bool {
        for existing in self.tags.iter() {
            if eq_trimmed_lower_ascii(&tag, &existing) {
                return true;
            }
        }
        false
    }

    pub fn is_overdue(&self, current_timestamp: u64) -> bool {
        current_timestamp > self.due_date
    }

    pub fn check_and_handle_expiration(
        &self,
        env: &Env,
        grace_period: u64,
    ) -> Result<bool, QuickLendXError> {
        if env.ledger().timestamp() <= self.grace_deadline(grace_period) {
            return Ok(false);
        }
        if self.status == InvoiceStatus::Funded {
            crate::defaults::handle_default(env, &self.id)?;
            return Ok(true);
        }
        Ok(self.is_overdue(env.ledger().timestamp()))
    }

    pub fn metadata(&self) -> Option<InvoiceMetadata> {
        if let (Some(customer_name), Some(customer_address), Some(tax_id), Some(notes)) = (
            self.metadata_customer_name.clone(),
            self.metadata_customer_address.clone(),
            self.metadata_tax_id.clone(),
            self.metadata_notes.clone(),
        ) {
            Some(InvoiceMetadata {
                customer_name,
                customer_address,
                tax_id,
                line_items: self.metadata_line_items.clone(),
                notes,
            })
        } else {
            None
        }
    }

    pub fn set_metadata(
        &mut self,
        env: &Env,
        metadata: Option<InvoiceMetadata>,
    ) -> Result<(), QuickLendXError> {
        match metadata {
            Some(metadata) => {
                self.metadata_customer_name = Some(metadata.customer_name);
                self.metadata_customer_address = Some(metadata.customer_address);
                self.metadata_tax_id = Some(metadata.tax_id);
                self.metadata_notes = Some(metadata.notes);
                self.metadata_line_items = metadata.line_items;
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

    pub fn update_metadata(
        &mut self,
        env: &Env,
        caller: &Address,
        metadata: InvoiceMetadata,
    ) -> Result<(), QuickLendXError> {
        if self.business != *caller {
            return Err(QuickLendXError::Unauthorized);
        }
        caller.require_auth();
        self.set_metadata(env, Some(metadata))
    }

    pub fn clear_metadata(&mut self, env: &Env, caller: &Address) -> Result<(), QuickLendXError> {
        if self.business != *caller {
            return Err(QuickLendXError::Unauthorized);
        }
        caller.require_auth();
        self.set_metadata(env, None)
    }

    pub fn cancel(&mut self, _env: &Env, actor: Address) -> Result<(), QuickLendXError> {
        if self.business != actor {
            return Err(QuickLendXError::Unauthorized);
        }
        self.status = InvoiceStatus::Cancelled;
        Ok(())
    }

    pub fn update_category(&mut self, category: InvoiceCategory) {
        self.category = category;
    }

    pub fn add_tag(&mut self, env: &Env, tag: String) -> Result<(), QuickLendXError> {
        let normalized = normalize_tag(env, &tag)?;
        if !self.has_tag(normalized.clone()) {
            if self.tags.len() >= MAX_INVOICE_TAGS {
                return Err(QuickLendXError::TagLimitExceeded);
            }
            self.tags.push_back(normalized);
        }
        Ok(())
    }

    pub fn remove_tag(&mut self, tag: String) -> Result<(), QuickLendXError> {
        let mut idx = 0u32;
        while idx < self.tags.len() {
            let existing = self.tags.get(idx).unwrap();
            if eq_trimmed_lower_ascii(&tag, &existing) {
                self.tags.remove(idx);
            } else {
                idx += 1;
            }
        }
        Ok(())
    }

    pub fn add_rating(
        &mut self,
        rating: u32,
        feedback: String,
        rater: Address,
        rated_at: u64,
    ) -> Result<(), QuickLendXError> {
        check_string_length(&feedback, MAX_FEEDBACK_LENGTH)?;
        if rating == 0 || rating > 5 {
            return Err(QuickLendXError::InvalidRating);
        }
        if self.status != InvoiceStatus::Funded && self.status != InvoiceStatus::Paid {
            return Err(QuickLendXError::NotFunded);
        }
        if self.ratings.len() >= MAX_RATINGS_PER_INVOICE {
            return Err(QuickLendXError::OperationNotAllowed);
        }
        for existing in self.ratings.iter() {
            if existing.rated_by == rater {
                return Err(QuickLendXError::AlreadyRated);
            }
        }
        self.ratings.push_back(InvoiceRating {
            rating,
            feedback,
            rated_by: rater,
            rated_at,
        });
        self.total_ratings = self.ratings.len();
        self.average_rating = Some(self.compute_average_rating());
        Ok(())
    }

    pub fn get_highest_rating(&self) -> Option<u32> {
        let mut highest: Option<u32> = None;
        for entry in self.ratings.iter() {
            highest = Some(highest.map_or(entry.rating, |v| v.max(entry.rating)));
        }
        highest
    }

    pub fn get_lowest_rating(&self) -> Option<u32> {
        let mut lowest: Option<u32> = None;
        for entry in self.ratings.iter() {
            lowest = Some(lowest.map_or(entry.rating, |v| v.min(entry.rating)));
        }
        lowest
    }

    pub fn grace_deadline(&self, grace_period: u64) -> u64 {
        self.due_date.saturating_add(grace_period)
    }

    fn compute_average_rating(&self) -> u32 {
        if self.ratings.is_empty() {
            return 0;
        }
        let mut total = 0u32;
        for entry in self.ratings.iter() {
            total = total.saturating_add(entry.rating);
        }
        total / self.ratings.len()
    }
}

fn eq_trimmed_lower_ascii(lhs: &String, rhs: &String) -> bool {
    const MAX_TAG_BYTES: usize = 50;

    let lhs_len = lhs.len() as usize;
    let rhs_len = rhs.len() as usize;
    if lhs_len > MAX_TAG_BYTES || rhs_len > MAX_TAG_BYTES {
        return false;
    }

    let mut lhs_buf = [0u8; MAX_TAG_BYTES];
    let mut rhs_buf = [0u8; MAX_TAG_BYTES];
    lhs.copy_into_slice(&mut lhs_buf[..lhs_len]);
    rhs.copy_into_slice(&mut rhs_buf[..rhs_len]);

    let mut lhs_start = 0usize;
    let mut lhs_end = lhs_len;
    while lhs_start < lhs_len && lhs_buf[lhs_start].is_ascii_whitespace() {
        lhs_start += 1;
    }
    while lhs_end > lhs_start && lhs_buf[lhs_end - 1].is_ascii_whitespace() {
        lhs_end -= 1;
    }

    if lhs_end - lhs_start != rhs_len {
        return false;
    }

    for idx in 0..rhs_len {
        if lhs_buf[lhs_start + idx].to_ascii_lowercase() != rhs_buf[idx].to_ascii_lowercase() {
            return false;
        }
    }
    true
}

fn zero_address(env: &Env) -> Address {
    Address::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    )
}
