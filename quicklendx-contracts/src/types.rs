//! Core data types for the QuickLendX invoice factoring protocol.
//!
//! This module defines all the fundamental types used throughout the contract,
//! including invoices, bids, investments, and their associated enums and structs.

use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Vec};
use crate::errors::QuickLendXError;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Time period enumeration for analytics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimePeriod {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

/// Invoice status enumeration representing the lifecycle of an invoice
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Pending,   // Invoice uploaded, awaiting verification
    Verified,  // Invoice verified and available for bidding
    Funded,    // Invoice has been funded by an investor
    Paid,      // Invoice has been paid and settled
    Defaulted, // Invoice payment is overdue/defaulted
    Cancelled, // Invoice has been cancelled by the business owner
    Refunded,  // Invoice has been refunded
}

/// Bid status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BidStatus {
    Placed,
    Withdrawn,
    Accepted,
    Expired,
    Cancelled,
}

/// Investment status enumeration
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvestmentStatus {
    Active,
    Withdrawn,
    Completed,
    Defaulted,
    Refunded,
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

/// Invoice category enumeration for classification
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceCategory {
    Services,
    Products,
    Consulting,
    Manufacturing,
    Technology,
    Healthcare,
    Other,
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Platform-wide analytics metrics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformMetrics {
    pub total_invoices: u32,
    pub total_investments: u32,
    pub total_volume: i128,
    pub total_fees_collected: i128,
    pub active_investors: u32,
    pub verified_businesses: u32,
    pub average_invoice_amount: i128,
    pub average_investment_amount: i128,
    pub platform_fee_rate: u32,
    pub default_rate: i128,
    pub success_rate: i128,
    pub timestamp: u64,
}

/// User behavior and risk metrics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserBehaviorMetrics {
    pub user_address: Address,
    pub total_invoices_uploaded: u32,
    pub total_investments_made: u32,
    pub total_bids_placed: u32,
    pub average_bid_amount: i128,
    pub average_investment_amount: i128,
    pub success_rate: u32,
    pub default_rate: u32,
    pub last_activity: u64,
    pub preferred_categories: Vec<InvoiceCategory>,
    pub risk_score: u32,
}

/// Platform performance and efficiency metrics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerformanceMetrics {
    pub platform_uptime: u64,
    pub average_settlement_time: u64,
    pub average_verification_time: u64,
    pub dispute_resolution_time: u64,
    pub system_response_time: u32,
    pub transaction_success_rate: u32,
    pub error_rate: u32,
    pub user_satisfaction_score: u32,
    pub platform_efficiency: u32,
}

/// Comprehensive business performance report
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessReport {
    pub report_id: BytesN<32>,
    pub business_address: Address,
    pub period: TimePeriod,
    pub start_date: u64,
    pub end_date: u64,
    pub invoices_uploaded: u32,
    pub invoices_funded: u32,
    pub total_volume: i128,
    pub average_funding_time: u64,
    pub success_rate: u32,
    pub default_rate: u32,
    pub category_breakdown: Vec<(InvoiceCategory, u32)>,
    pub rating_average: Option<u32>,
    pub total_ratings: u32,
    pub generated_at: u64,
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
    pub amount: i128,
    pub timestamp: u64,
    pub transaction_id: String,
}

/// Dispute structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub created_by: Address,
    pub created_at: u64,
    pub reason: String,
    pub evidence: String,
    pub resolution: String,
    pub resolved_by: Address,
    pub resolved_at: u64,
}

/// Invoice rating structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceRating {
    pub rating: u32,
    pub feedback: String,
    pub rated_by: Address,
    pub rated_at: u64,
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

/// Core invoice data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Invoice {
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub status: InvoiceStatus,
    pub created_at: u64,
    pub description: String,
    pub metadata_customer_name: Option<String>,
    pub metadata_customer_address: Option<String>,
    pub metadata_tax_id: Option<String>,
    pub metadata_notes: Option<String>,
    pub metadata_line_items: Vec<LineItemRecord>,
    pub category: InvoiceCategory,
    pub tags: Vec<String>,
    pub funded_amount: i128,
    pub funded_at: Option<u64>,
    pub investor: Option<Address>,
    pub settled_at: Option<u64>,
    pub average_rating: Option<u32>,
    pub total_ratings: u32,
    pub ratings: Vec<InvoiceRating>,
    pub dispute_status: DisputeStatus,
    pub dispute: Dispute,
    pub total_paid: i128,
    pub payment_history: Vec<PaymentRecord>,
}

/// Bid data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bid {
    pub bid_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub timestamp: u64,
    pub status: BidStatus,
    pub expiration_timestamp: u64,
}

/// Insurance coverage structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceCoverage {
    pub provider: Address,
    pub coverage_amount: i128,
    pub premium_amount: i128,
    pub coverage_percentage: u32,
    pub active: bool,
}

/// Investment data structure
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub funded_at: u64,
    pub status: InvestmentStatus,
    pub insurance: Vec<InsuranceCoverage>,
}

/// Platform fee configuration stored on-chain
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformFeeConfig {
    pub fee_bps: u32,
    pub treasury_address: Option<Address>,
    pub updated_at: u64,
    pub updated_by: Address,
}

// ---------------------------------------------------------------------------
// Implementations
// ---------------------------------------------------------------------------

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
        let mut id_bytes = [0u8; 32];
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        id_bytes[0..8].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[8..12].copy_from_slice(&sequence.to_be_bytes());
        let id = BytesN::from_array(env, &id_bytes);

        Ok(Self {
            id,
            business,
            amount,
            currency,
            due_date,
            status: InvoiceStatus::Pending,
            created_at: timestamp,
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
            ratings: Vec::new(env),
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: Address::from_string(&String::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")),
                created_at: 0,
                reason: String::from_str(env, ""),
                evidence: String::from_str(env, ""),
                resolution: String::from_str(env, ""),
                resolved_by: Address::from_string(&String::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF")),
                resolved_at: 0,
            },
            total_paid: 0,
            payment_history: Vec::new(env),
        })
    }

    pub fn verify(&mut self, _env: &Env, _admin: Address) {
        self.status = InvoiceStatus::Verified;
    }

    pub fn cancel(&mut self, _env: &Env, _business: Address) -> Result<(), QuickLendXError> {
        if self.status != InvoiceStatus::Pending && self.status != InvoiceStatus::Verified {
            return Err(QuickLendXError::InvalidStatus);
        }
        self.status = InvoiceStatus::Cancelled;
        Ok(())
    }

    pub fn mark_as_funded(&mut self, _env: &Env, investor: Address, amount: i128, timestamp: u64) {
        self.status = InvoiceStatus::Funded;
        self.investor = Some(investor);
        self.funded_amount = amount;
        self.funded_at = Some(timestamp);
    }

    pub fn mark_as_paid(&mut self, _env: &Env, _business: Address, timestamp: u64) {
        self.status = InvoiceStatus::Paid;
        self.settled_at = Some(timestamp);
    }

    pub fn mark_as_defaulted(&mut self) {
        self.status = InvoiceStatus::Defaulted;
    }

    pub fn is_overdue(&self, current_timestamp: u64) -> bool {
        current_timestamp > self.due_date
    }

    pub fn grace_deadline(&self, grace_period_seconds: u64) -> u64 {
        self.due_date.saturating_add(grace_period_seconds)
    }

    pub fn check_and_handle_expiration(&self, _env: &Env, _grace_period: u64) -> Result<(), QuickLendXError> {
        // Implementation logic...
        Ok(())
    }

    pub fn metadata(&self) -> Option<InvoiceMetadata> {
        if self.metadata_customer_name.is_none() { return None; }
        Some(InvoiceMetadata {
            customer_name: self.metadata_customer_name.clone().unwrap(),
            customer_address: self.metadata_customer_address.clone().unwrap_or(String::from_str(self.business.env(), "")),
            tax_id: self.metadata_tax_id.clone().unwrap_or(String::from_str(self.business.env(), "")),
            line_items: self.metadata_line_items.clone(),
            notes: self.metadata_notes.clone().unwrap_or(String::from_str(self.business.env(), "")),
        })
    }

    pub fn set_metadata(&mut self, _env: &Env, metadata: Option<InvoiceMetadata>) -> Result<(), QuickLendXError> {
        if let Some(m) = metadata {
            self.metadata_customer_name = Some(m.customer_name);
            self.metadata_customer_address = Some(m.customer_address);
            self.metadata_tax_id = Some(m.tax_id);
            self.metadata_line_items = m.line_items;
            self.metadata_notes = Some(m.notes);
        } else {
            self.metadata_customer_name = None;
            self.metadata_customer_address = None;
            self.metadata_tax_id = None;
            self.metadata_line_items = Vec::new(self.business.env());
            self.metadata_notes = None;
        }
        Ok(())
    }

    pub fn add_tag(&mut self, env: &Env, tag: String) -> Result<(), QuickLendXError> {
        self.business.require_auth();
        crate::verification::validate_invoice_tags(env, &self.tags)?;
        let normalized = crate::verification::normalize_tag(env, &tag)?;

        // Check for duplicates
        for existing_tag in self.tags.iter() {
            if existing_tag == normalized {
                return Err(QuickLendXError::InvalidTag);
            }
        }

        self.tags.push_back(normalized);
        Ok(())
    }

    pub fn remove_tag(&mut self, env: &Env, tag: String) -> Result<(), QuickLendXError> {
        self.business.require_auth();
        let normalized = crate::verification::normalize_tag(env, &tag)?;

        let mut new_tags = soroban_sdk::Vec::new(env);
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
        Ok(())
    }
}

impl Investment {
    pub fn process_insurance_claim(&self) -> Option<(Address, i128)> {
        if self.insurance.is_empty() {
            return None;
        }
        let env = self.investor.env();
        let mut total_coverage = 0i128;
        let mut primary_provider = None;

        for coverage in self.insurance.iter() {
            if coverage.active {
                total_coverage = total_coverage.saturating_add(coverage.coverage_amount);
                if primary_provider.is_none() {
                    primary_provider = Some(coverage.provider.clone());
                }
            }
        }

        if total_coverage > 0 && primary_provider.is_some() {
            Some((primary_provider.unwrap(), total_coverage))
        } else {
            None
        }
    }
}
