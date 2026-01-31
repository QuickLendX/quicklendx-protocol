use crate::bid::{BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::invoice::{Invoice, InvoiceMetadata};
use soroban_sdk::{contracttype, symbol_short, vec, Address, Env, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BusinessVerificationStatus {
    Pending,
    Verified,
    Rejected,
}

#[contracttype]
pub struct BusinessVerification {
    pub business: Address,
    pub status: BusinessVerificationStatus,
    pub verified_at: Option<u64>,
    pub verified_by: Option<Address>,
    pub kyc_data: String, // Encrypted KYC data
    pub submitted_at: u64,
    pub rejection_reason: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum InvestorTier {
    Basic,
    Silver,
    Gold,
    Platinum,
    VIP,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum InvestorRiskLevel {
    Low,
    Medium,
    High,
    VeryHigh,
}

#[contracttype]
pub struct InvestorVerification {
    pub investor: Address,
    pub status: BusinessVerificationStatus,
    pub verified_at: Option<u64>,
    pub verified_by: Option<Address>,
    pub kyc_data: String,
    pub investment_limit: i128,
    pub submitted_at: u64,
    pub tier: InvestorTier,
    pub risk_level: InvestorRiskLevel,
    pub risk_score: u32,
    pub total_invested: i128,
    pub total_returns: i128,
    pub successful_investments: u32,
    pub defaulted_investments: u32,
    pub last_activity: u64,
    pub rejection_reason: Option<String>,
    pub compliance_notes: Option<String>,
}

const MIN_BID_AMOUNT: i128 = 100;

pub struct BusinessVerificationStorage;

impl BusinessVerificationStorage {
    const VERIFIED_BUSINESSES_KEY: &'static str = "verified_businesses";
    const PENDING_BUSINESSES_KEY: &'static str = "pending_businesses";
    const REJECTED_BUSINESSES_KEY: &'static str = "rejected_businesses";
    const ADMIN_KEY: &'static str = "admin_address";

    pub fn store_verification(env: &Env, verification: &BusinessVerification) {
        env.storage()
            .instance()
            .set(&verification.business, verification);

        // Add to status-specific lists
        match verification.status {
            BusinessVerificationStatus::Verified => {
                Self::add_to_verified_businesses(env, &verification.business);
            }
            BusinessVerificationStatus::Pending => {
                Self::add_to_pending_businesses(env, &verification.business);
            }
            BusinessVerificationStatus::Rejected => {
                Self::add_to_rejected_businesses(env, &verification.business);
            }
        }
    }

    pub fn get_verification(env: &Env, business: &Address) -> Option<BusinessVerification> {
        env.storage().instance().get(business)
    }

    pub fn update_verification(env: &Env, verification: &BusinessVerification) {
        let old_verification = Self::get_verification(env, &verification.business);

        // Remove from old status list
        if let Some(old_ver) = old_verification {
            match old_ver.status {
                BusinessVerificationStatus::Verified => {
                    Self::remove_from_verified_businesses(env, &verification.business);
                }
                BusinessVerificationStatus::Pending => {
                    Self::remove_from_pending_businesses(env, &verification.business);
                }
                BusinessVerificationStatus::Rejected => {
                    Self::remove_from_rejected_businesses(env, &verification.business);
                }
            }
        }

        // Store new verification
        Self::store_verification(env, verification);
    }

    pub fn is_business_verified(env: &Env, business: &Address) -> bool {
        if let Some(verification) = Self::get_verification(env, business) {
            matches!(verification.status, BusinessVerificationStatus::Verified)
        } else {
            false
        }
    }

    pub fn get_verified_businesses(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::VERIFIED_BUSINESSES_KEY)
            .unwrap_or(vec![env])
    }

    pub fn get_pending_businesses(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::PENDING_BUSINESSES_KEY)
            .unwrap_or(vec![env])
    }

    pub fn get_rejected_businesses(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::REJECTED_BUSINESSES_KEY)
            .unwrap_or(vec![env])
    }

    fn add_to_verified_businesses(env: &Env, business: &Address) {
        let mut verified = Self::get_verified_businesses(env);
        verified.push_back(business.clone());
        env.storage()
            .instance()
            .set(&Self::VERIFIED_BUSINESSES_KEY, &verified);
    }

    fn add_to_pending_businesses(env: &Env, business: &Address) {
        let mut pending = Self::get_pending_businesses(env);
        pending.push_back(business.clone());
        env.storage()
            .instance()
            .set(&Self::PENDING_BUSINESSES_KEY, &pending);
    }

    fn add_to_rejected_businesses(env: &Env, business: &Address) {
        let mut rejected = Self::get_rejected_businesses(env);
        rejected.push_back(business.clone());
        env.storage()
            .instance()
            .set(&Self::REJECTED_BUSINESSES_KEY, &rejected);
    }

    fn remove_from_verified_businesses(env: &Env, business: &Address) {
        let verified = Self::get_verified_businesses(env);
        let mut new_verified = vec![env];
        for addr in verified.iter() {
            if addr != *business {
                new_verified.push_back(addr);
            }
        }
        env.storage()
            .instance()
            .set(&Self::VERIFIED_BUSINESSES_KEY, &new_verified);
    }

    fn remove_from_pending_businesses(env: &Env, business: &Address) {
        let pending = Self::get_pending_businesses(env);
        let mut new_pending = vec![env];
        for addr in pending.iter() {
            if addr != *business {
                new_pending.push_back(addr);
            }
        }
        env.storage()
            .instance()
            .set(&Self::PENDING_BUSINESSES_KEY, &new_pending);
    }

    fn remove_from_rejected_businesses(env: &Env, business: &Address) {
        let rejected = Self::get_rejected_businesses(env);
        let mut new_rejected = vec![env];
        for addr in rejected.iter() {
            if addr != *business {
                new_rejected.push_back(addr);
            }
        }
        env.storage()
            .instance()
            .set(&Self::REJECTED_BUSINESSES_KEY, &new_rejected);
    }

    /// @deprecated Use `admin::AdminStorage::initialize()` or `admin::AdminStorage::set_admin()` instead
    /// This function is kept for backward compatibility with existing tests.
    /// It syncs with the new AdminStorage system.
    pub fn set_admin(env: &Env, admin: &Address) {
        // Store in old location for backward compatibility
        env.storage().instance().set(&Self::ADMIN_KEY, admin);

        // Always sync with new AdminStorage
        // This allows tests that call set_admin() multiple times to work
        env.storage()
            .instance()
            .set(&crate::admin::ADMIN_KEY, admin);
        env.storage()
            .instance()
            .set(&crate::admin::ADMIN_INITIALIZED_KEY, &true);
    }

    /// @deprecated Use `admin::AdminStorage::get_admin()` instead
    /// This function is kept for backward compatibility only
    pub fn get_admin(env: &Env) -> Option<Address> {
        // Try new storage first, fall back to old
        crate::admin::AdminStorage::get_admin(env)
            .or_else(|| env.storage().instance().get(&Self::ADMIN_KEY))
    }

    /// @deprecated Use `admin::AdminStorage::is_admin()` instead
    /// This function is kept for backward compatibility only
    pub fn is_admin(env: &Env, address: &Address) -> bool {
        crate::admin::AdminStorage::is_admin(env, address)
    }
}

pub struct InvestorVerificationStorage;

impl InvestorVerificationStorage {
    const VERIFIED_INVESTORS_KEY: &'static str = "verified_investors";
    const PENDING_INVESTORS_KEY: &'static str = "pending_investors";
    const REJECTED_INVESTORS_KEY: &'static str = "rejected_investors";
    const INVESTOR_HISTORY_KEY: &'static str = "investor_history";
    const INVESTOR_ANALYTICS_KEY: &'static str = "investor_analytics";

    pub fn submit(env: &Env, investor: &Address, kyc_data: String) -> Result<(), QuickLendXError> {
        let mut verification = Self::get(env, investor);
        match verification {
            Some(ref existing) => match existing.status {
                BusinessVerificationStatus::Pending => {
                    return Err(QuickLendXError::KYCAlreadyPending)
                }
                BusinessVerificationStatus::Verified => {
                    return Err(QuickLendXError::KYCAlreadyVerified)
                }
                BusinessVerificationStatus::Rejected => {
                    verification = Some(InvestorVerification {
                        investor: investor.clone(),
                        status: BusinessVerificationStatus::Pending,
                        verified_at: None,
                        verified_by: None,
                        kyc_data,
                        investment_limit: existing.investment_limit,
                        submitted_at: env.ledger().timestamp(),
                        tier: existing.tier.clone(),
                        risk_level: existing.risk_level.clone(),
                        risk_score: existing.risk_score,
                        total_invested: existing.total_invested,
                        total_returns: existing.total_returns,
                        successful_investments: existing.successful_investments,
                        defaulted_investments: existing.defaulted_investments,
                        last_activity: existing.last_activity,
                        rejection_reason: None,
                        compliance_notes: None,
                    });
                }
            },
            None => {
                verification = Some(InvestorVerification {
                    investor: investor.clone(),
                    status: BusinessVerificationStatus::Pending,
                    verified_at: None,
                    verified_by: None,
                    kyc_data,
                    investment_limit: 0,
                    submitted_at: env.ledger().timestamp(),
                    tier: InvestorTier::Basic,
                    risk_level: InvestorRiskLevel::High, // Default to high risk for new investors
                    risk_score: 100,                     // Default high risk score
                    total_invested: 0,
                    total_returns: 0,
                    successful_investments: 0,
                    defaulted_investments: 0,
                    last_activity: env.ledger().timestamp(),
                    rejection_reason: None,
                    compliance_notes: None,
                });
            }
        }

        if let Some(v) = verification {
            Self::store(env, &v);
            Self::add_to_pending_investors(env, investor);
        }
        Ok(())
    }

    pub fn store(env: &Env, verification: &InvestorVerification) {
        env.storage()
            .instance()
            .set(&verification.investor, verification);
    }

    pub fn get(env: &Env, investor: &Address) -> Option<InvestorVerification> {
        env.storage().instance().get(investor)
    }

    pub fn update(env: &Env, verification: &InvestorVerification) {
        let old_verification = Self::get(env, &verification.investor);

        // Remove from old status list
        if let Some(old_ver) = old_verification {
            match old_ver.status {
                BusinessVerificationStatus::Verified => {
                    Self::remove_from_verified_investors(env, &verification.investor);
                }
                BusinessVerificationStatus::Pending => {
                    Self::remove_from_pending_investors(env, &verification.investor);
                }
                BusinessVerificationStatus::Rejected => {
                    Self::remove_from_rejected_investors(env, &verification.investor);
                }
            }
        }

        // Store new verification
        Self::store(env, verification);

        // Add to new status list
        match verification.status {
            BusinessVerificationStatus::Verified => {
                Self::add_to_verified_investors(env, &verification.investor);
            }
            BusinessVerificationStatus::Pending => {
                Self::add_to_pending_investors(env, &verification.investor);
            }
            BusinessVerificationStatus::Rejected => {
                Self::add_to_rejected_investors(env, &verification.investor);
            }
        }
    }

    pub fn is_investor_verified(env: &Env, investor: &Address) -> bool {
        if let Some(verification) = Self::get(env, investor) {
            matches!(verification.status, BusinessVerificationStatus::Verified)
        } else {
            false
        }
    }

    pub fn get_verified_investors(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::VERIFIED_INVESTORS_KEY)
            .unwrap_or(vec![env])
    }

    pub fn get_pending_investors(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::PENDING_INVESTORS_KEY)
            .unwrap_or(vec![env])
    }

    pub fn get_rejected_investors(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::REJECTED_INVESTORS_KEY)
            .unwrap_or(vec![env])
    }

    pub fn get_investors_by_tier(env: &Env, tier: InvestorTier) -> Vec<Address> {
        let verified_investors = Self::get_verified_investors(env);
        let mut tier_investors = Vec::new(env);

        for investor in verified_investors.iter() {
            if let Some(verification) = Self::get(env, &investor) {
                if verification.tier == tier {
                    tier_investors.push_back(investor);
                }
            }
        }

        tier_investors
    }

    pub fn get_investors_by_risk_level(env: &Env, risk_level: InvestorRiskLevel) -> Vec<Address> {
        let verified_investors = Self::get_verified_investors(env);
        let mut risk_investors = Vec::new(env);

        for investor in verified_investors.iter() {
            if let Some(verification) = Self::get(env, &investor) {
                if verification.risk_level == risk_level {
                    risk_investors.push_back(investor);
                }
            }
        }

        risk_investors
    }

    fn add_to_verified_investors(env: &Env, investor: &Address) {
        let mut verified = Self::get_verified_investors(env);
        verified.push_back(investor.clone());
        env.storage()
            .instance()
            .set(&Self::VERIFIED_INVESTORS_KEY, &verified);
    }

    fn add_to_pending_investors(env: &Env, investor: &Address) {
        let mut pending = Self::get_pending_investors(env);
        pending.push_back(investor.clone());
        env.storage()
            .instance()
            .set(&Self::PENDING_INVESTORS_KEY, &pending);
    }

    fn add_to_rejected_investors(env: &Env, investor: &Address) {
        let mut rejected = Self::get_rejected_investors(env);
        rejected.push_back(investor.clone());
        env.storage()
            .instance()
            .set(&Self::REJECTED_INVESTORS_KEY, &rejected);
    }

    fn remove_from_verified_investors(env: &Env, investor: &Address) {
        let verified = Self::get_verified_investors(env);
        let mut new_verified = vec![env];
        for addr in verified.iter() {
            if addr != *investor {
                new_verified.push_back(addr);
            }
        }
        env.storage()
            .instance()
            .set(&Self::VERIFIED_INVESTORS_KEY, &new_verified);
    }

    fn remove_from_pending_investors(env: &Env, investor: &Address) {
        let pending = Self::get_pending_investors(env);
        let mut new_pending = vec![env];
        for addr in pending.iter() {
            if addr != *investor {
                new_pending.push_back(addr);
            }
        }
        env.storage()
            .instance()
            .set(&Self::PENDING_INVESTORS_KEY, &new_pending);
    }

    fn remove_from_rejected_investors(env: &Env, investor: &Address) {
        let rejected = Self::get_rejected_investors(env);
        let mut new_rejected = vec![env];
        for addr in rejected.iter() {
            if addr != *investor {
                new_rejected.push_back(addr);
            }
        }
        env.storage()
            .instance()
            .set(&Self::REJECTED_INVESTORS_KEY, &new_rejected);
    }
}

pub fn validate_bid(
    env: &Env,
    invoice: &Invoice,
    bid_amount: i128,
    expected_return: i128,
    investor: &Address,
) -> Result<(), QuickLendXError> {
    if bid_amount <= 0 || bid_amount < MIN_BID_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }

    if bid_amount > invoice.amount {
        return Err(QuickLendXError::InvoiceAmountInvalid);
    }

    if expected_return <= bid_amount {
        return Err(QuickLendXError::InvalidAmount);
    }

    // Validate investor can make this investment
    validate_investor_investment(env, investor, bid_amount)?;

    BidStorage::cleanup_expired_bids(env, &invoice.id);
    let existing_bids = BidStorage::get_bids_for_invoice(env, &invoice.id);
    for bid_id in existing_bids.iter() {
        if let Some(existing_bid) = BidStorage::get_bid(env, &bid_id) {
            if existing_bid.investor == *investor && existing_bid.status == BidStatus::Placed {
                return Err(QuickLendXError::OperationNotAllowed);
            }
        }
    }

    Ok(())
}

pub fn submit_kyc_application(
    env: &Env,
    business: &Address,
    kyc_data: String,
) -> Result<(), QuickLendXError> {
    // Only the business can submit their own KYC
    business.require_auth();

    // Check if business already has a verification record
    if let Some(existing_verification) =
        BusinessVerificationStorage::get_verification(env, business)
    {
        match existing_verification.status {
            BusinessVerificationStatus::Pending => {
                return Err(QuickLendXError::KYCAlreadyPending);
            }
            BusinessVerificationStatus::Verified => {
                return Err(QuickLendXError::KYCAlreadyVerified);
            }
            BusinessVerificationStatus::Rejected => {
                // Allow resubmission if previously rejected
            }
        }
    }

    let verification = BusinessVerification {
        business: business.clone(),
        status: BusinessVerificationStatus::Pending,
        verified_at: None,
        verified_by: None,
        kyc_data,
        submitted_at: env.ledger().timestamp(),
        rejection_reason: None,
    };

    BusinessVerificationStorage::store_verification(env, &verification);
    emit_kyc_submitted(env, business);
    Ok(())
}

pub fn verify_business(
    env: &Env,
    admin: &Address,
    business: &Address,
) -> Result<(), QuickLendXError> {
    // Only admin can verify businesses
    admin.require_auth();
    if !BusinessVerificationStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    let mut verification = BusinessVerificationStorage::get_verification(env, business)
        .ok_or(QuickLendXError::KYCNotFound)?;

    if !matches!(verification.status, BusinessVerificationStatus::Pending) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    verification.status = BusinessVerificationStatus::Verified;
    verification.verified_at = Some(env.ledger().timestamp());
    verification.verified_by = Some(admin.clone());

    BusinessVerificationStorage::update_verification(env, &verification);
    emit_business_verified(env, business, admin);
    Ok(())
}

pub fn reject_business(
    env: &Env,
    admin: &Address,
    business: &Address,
    reason: String,
) -> Result<(), QuickLendXError> {
    // Only admin can reject businesses
    admin.require_auth();
    if !BusinessVerificationStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    let mut verification = BusinessVerificationStorage::get_verification(env, business)
        .ok_or(QuickLendXError::KYCNotFound)?;

    if !matches!(verification.status, BusinessVerificationStatus::Pending) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    verification.status = BusinessVerificationStatus::Rejected;
    verification.rejection_reason = Some(reason);

    BusinessVerificationStorage::update_verification(env, &verification);
    emit_business_rejected(env, business, admin);
    Ok(())
}

pub fn get_business_verification_status(
    env: &Env,
    business: &Address,
) -> Option<BusinessVerification> {
    BusinessVerificationStorage::get_verification(env, business)
}

pub fn require_business_verification(env: &Env, business: &Address) -> Result<(), QuickLendXError> {
    if !BusinessVerificationStorage::is_business_verified(env, business) {
        return Err(QuickLendXError::BusinessNotVerified);
    }
    Ok(())
}

// Keep the existing invoice verification function
pub fn verify_invoice_data(
    env: &Env,
    _business: &Address,
    amount: i128,
    _currency: &Address,
    due_date: u64,
    description: &String,
) -> Result<(), QuickLendXError> {
    // First check if business is verified (temporarily disabled for debugging)
    // require_business_verification(env, business)?;

    if amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }
    let current_timestamp = env.ledger().timestamp();
    if due_date <= current_timestamp {
        return Err(QuickLendXError::InvoiceDueDateInvalid);
    }
    if description.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }
    Ok(())
}

// Event emission functions (from main)
fn emit_kyc_submitted(env: &Env, business: &Address) {
    env.events().publish(
        (symbol_short!("kyc_sub"),),
        (business.clone(), env.ledger().timestamp()),
    );
}

fn emit_business_verified(env: &Env, business: &Address, admin: &Address) {
    env.events().publish(
        (symbol_short!("bus_ver"),),
        (business.clone(), admin.clone(), env.ledger().timestamp()),
    );
}

fn emit_business_rejected(env: &Env, business: &Address, admin: &Address) {
    env.events().publish(
        (symbol_short!("bus_rej"),),
        (business.clone(), admin.clone()),
    );
}

/// Validate invoice category
pub fn validate_invoice_category(
    category: &crate::invoice::InvoiceCategory,
) -> Result<(), QuickLendXError> {
    // All categories are valid as they are defined in the enum
    // This function can be extended to add additional validation logic if needed
    match category {
        crate::invoice::InvoiceCategory::Services => Ok(()),
        crate::invoice::InvoiceCategory::Products => Ok(()),
        crate::invoice::InvoiceCategory::Consulting => Ok(()),
        crate::invoice::InvoiceCategory::Manufacturing => Ok(()),
        crate::invoice::InvoiceCategory::Technology => Ok(()),
        crate::invoice::InvoiceCategory::Healthcare => Ok(()),
        crate::invoice::InvoiceCategory::Other => Ok(()),
    }
}

/// Validate invoice tags
pub fn validate_invoice_tags(tags: &Vec<String>) -> Result<(), QuickLendXError> {
    // Check tag count limit (max 10 tags per invoice)
    if tags.len() > 10 {
        return Err(QuickLendXError::TagLimitExceeded);
    }

    // Validate each tag
    for tag in tags.iter() {
        // Check tag length (1-50 characters)
        if tag.len() < 1 || tag.len() > 50 {
            return Err(QuickLendXError::InvalidTag);
        }

        // Check for empty tags (length 0 is already checked above)
        // Note: Soroban String doesn't have trim() method
    }

    Ok(())
}

pub fn submit_investor_kyc(
    env: &Env,
    investor: &Address,
    kyc_data: String,
) -> Result<(), QuickLendXError> {
    investor.require_auth();
    InvestorVerificationStorage::submit(env, investor, kyc_data)
}

pub fn verify_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
    investment_limit: i128,
) -> Result<InvestorVerification, QuickLendXError> {
    admin.require_auth();

    if investment_limit <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    match verification.status {
        BusinessVerificationStatus::Verified => return Err(QuickLendXError::KYCAlreadyVerified),
        BusinessVerificationStatus::Pending | BusinessVerificationStatus::Rejected => {
            // Calculate risk score and determine tier
            let risk_score = calculate_investor_risk_score(env, investor, &verification.kyc_data)?;
            let tier = determine_investor_tier(env, investor, risk_score)?;
            let risk_level = determine_risk_level(risk_score);

            // Calculate final investment limit based on tier and risk
            let calculated_limit = calculate_investment_limit(&tier, &risk_level, investment_limit);

            verification.status = BusinessVerificationStatus::Verified;
            verification.verified_at = Some(env.ledger().timestamp());
            verification.verified_by = Some(admin.clone());
            verification.investment_limit = calculated_limit;
            verification.tier = tier;
            verification.risk_level = risk_level;
            verification.risk_score = risk_score;
            verification.compliance_notes = Some(String::from_str(env, "Verified by admin"));

            InvestorVerificationStorage::update(env, &verification);
            Ok(verification)
        }
    }
}

pub fn reject_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
    reason: String,
) -> Result<(), QuickLendXError> {
    admin.require_auth();
    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    verification.status = BusinessVerificationStatus::Rejected;
    verification.verified_at = Some(env.ledger().timestamp());
    verification.verified_by = Some(admin.clone());
    verification.rejection_reason = Some(reason);
    verification.compliance_notes = Some(String::from_str(env, "Rejected by admin"));

    InvestorVerificationStorage::update(env, &verification);
    Ok(())
}

pub fn get_investor_verification(env: &Env, investor: &Address) -> Option<InvestorVerification> {
    InvestorVerificationStorage::get(env, investor)
}

/// Calculate investor risk score based on various factors
pub fn calculate_investor_risk_score(
    env: &Env,
    investor: &Address,
    kyc_data: &String,
) -> Result<u32, QuickLendXError> {
    let mut risk_score = 0u32;

    // Base risk score from KYC data analysis (simplified)
    // In a real implementation, this would analyze the KYC data
    let kyc_length = kyc_data.len();
    if kyc_length < 100 {
        risk_score += 30; // High risk for incomplete KYC
    } else if kyc_length < 500 {
        risk_score += 20; // Medium risk
    } else {
        risk_score += 10; // Lower risk for comprehensive KYC
    }

    // Check investment history if available
    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        let total_investments =
            verification.successful_investments + verification.defaulted_investments;

        if total_investments > 0 {
            let default_rate = (verification.defaulted_investments * 100) / total_investments;
            risk_score += default_rate;
        }

        // Adjust based on total invested amount
        if verification.total_invested > 1000000 {
            // 1M+ invested
            risk_score = risk_score.saturating_sub(20);
        } else if verification.total_invested > 100000 {
            // 100K+ invested
            risk_score = risk_score.saturating_sub(10);
        }
    }

    // Cap risk score at 100
    if risk_score > 100 {
        risk_score = 100;
    }

    Ok(risk_score)
}

/// Determine investor tier based on risk score and investment history
pub fn determine_investor_tier(
    env: &Env,
    investor: &Address,
    risk_score: u32,
) -> Result<InvestorTier, QuickLendXError> {
    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        let total_invested = verification.total_invested;
        let successful_investments = verification.successful_investments;

        // VIP tier: Very low risk, high investment volume, many successful investments
        if risk_score <= 10 && total_invested > 5000000 && successful_investments > 50 {
            return Ok(InvestorTier::VIP);
        }

        // Platinum tier: Low risk, high investment volume
        if risk_score <= 20 && total_invested > 1000000 && successful_investments > 20 {
            return Ok(InvestorTier::Platinum);
        }

        // Gold tier: Medium-low risk, moderate investment volume
        if risk_score <= 40 && total_invested > 100000 && successful_investments > 10 {
            return Ok(InvestorTier::Gold);
        }

        // Silver tier: Medium risk, some investment history
        if risk_score <= 60 && total_invested > 10000 && successful_investments > 3 {
            return Ok(InvestorTier::Silver);
        }
    }

    // Default to Basic tier
    Ok(InvestorTier::Basic)
}

/// Determine risk level based on risk score
pub fn determine_risk_level(risk_score: u32) -> InvestorRiskLevel {
    match risk_score {
        0..=25 => InvestorRiskLevel::Low,
        26..=50 => InvestorRiskLevel::Medium,
        51..=75 => InvestorRiskLevel::High,
        _ => InvestorRiskLevel::VeryHigh,
    }
}

/// Calculate investment limit based on tier and risk level
pub fn calculate_investment_limit(
    tier: &InvestorTier,
    risk_level: &InvestorRiskLevel,
    base_limit: i128,
) -> i128 {
    let tier_multiplier = match tier {
        InvestorTier::VIP => 10,
        InvestorTier::Platinum => 5,
        InvestorTier::Gold => 3,
        InvestorTier::Silver => 2,
        InvestorTier::Basic => 1,
    };

    let risk_multiplier = match risk_level {
        InvestorRiskLevel::Low => 100,     // 100% of calculated limit
        InvestorRiskLevel::Medium => 75,   // 75% of calculated limit
        InvestorRiskLevel::High => 50,     // 50% of calculated limit
        InvestorRiskLevel::VeryHigh => 25, // 25% of calculated limit
    };

    let calculated_limit = base_limit.saturating_mul(tier_multiplier);
    calculated_limit
        .saturating_mul(risk_multiplier)
        .saturating_div(100)
}

/// Update investor analytics after an investment
pub fn update_investor_analytics(
    env: &Env,
    investor: &Address,
    investment_amount: i128,
    is_successful: bool,
) -> Result<(), QuickLendXError> {
    if let Some(mut verification) = InvestorVerificationStorage::get(env, investor) {
        verification.total_invested = verification
            .total_invested
            .saturating_add(investment_amount);
        verification.last_activity = env.ledger().timestamp();

        if is_successful {
            verification.successful_investments =
                verification.successful_investments.saturating_add(1);
            // Calculate returns (simplified - would need actual return data)
            let estimated_return = investment_amount.saturating_mul(110).saturating_div(100); // 10% return
            verification.total_returns =
                verification.total_returns.saturating_add(estimated_return);
        } else {
            verification.defaulted_investments =
                verification.defaulted_investments.saturating_add(1);
        }

        // Recalculate risk score and tier
        verification.risk_score =
            calculate_investor_risk_score(env, investor, &verification.kyc_data)?;
        verification.risk_level = determine_risk_level(verification.risk_score);
        verification.tier = determine_investor_tier(env, investor, verification.risk_score)?;

        // Update investment limit based on new tier and risk
        let base_limit = 100000; // Base limit of 100K
        verification.investment_limit =
            calculate_investment_limit(&verification.tier, &verification.risk_level, base_limit);

        InvestorVerificationStorage::update(env, &verification);
    }

    Ok(())
}

/// Get investor analytics summary
pub fn get_investor_analytics(
    env: &Env,
    investor: &Address,
) -> Result<InvestorVerification, QuickLendXError> {
    InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)
}

/// Validate investor can make investment based on limits and risk
pub fn validate_investor_investment(
    env: &Env,
    investor: &Address,
    investment_amount: i128,
) -> Result<(), QuickLendXError> {
    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        // Check if investor is verified
        if !matches!(verification.status, BusinessVerificationStatus::Verified) {
            return Err(QuickLendXError::BusinessNotVerified);
        }

        // Check investment limit
        if investment_amount > verification.investment_limit {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Check risk level restrictions
        match verification.risk_level {
            InvestorRiskLevel::VeryHigh => {
                // Very high risk investors have additional restrictions
                if investment_amount > 10000 {
                    return Err(QuickLendXError::InvalidAmount);
                }
            }
            InvestorRiskLevel::High => {
                // High risk investors have moderate restrictions
                if investment_amount > 50000 {
                    return Err(QuickLendXError::InvalidAmount);
                }
            }
            _ => {
                // Medium and low risk investors can invest up to their limit
            }
        }

        Ok(())
    } else {
        Err(QuickLendXError::KYCNotFound)
    }
}

/// Set investment limit for a verified investor (admin only)
pub fn set_investment_limit(
    env: &Env,
    admin: &Address,
    investor: &Address,
    new_limit: i128,
) -> Result<(), QuickLendXError> {
    admin.require_auth();

    // Check admin authorization
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    if new_limit <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    // Only allow setting limits for verified investors
    if !matches!(verification.status, BusinessVerificationStatus::Verified) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

    // Calculate final investment limit based on tier and risk
    let calculated_limit =
        calculate_investment_limit(&verification.tier, &verification.risk_level, new_limit);

    verification.investment_limit = calculated_limit;
    verification.compliance_notes =
        Some(String::from_str(env, "Investment limit updated by admin"));

    InvestorVerificationStorage::update(env, &verification);
    Ok(())
}

/// Validate structured invoice metadata against the invoice amount
pub fn validate_invoice_metadata(
    metadata: &InvoiceMetadata,
    invoice_amount: i128,
) -> Result<(), QuickLendXError> {
    if metadata.customer_name.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    if metadata.customer_address.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    if metadata.tax_id.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    if metadata.line_items.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    let mut computed_total = 0i128;
    for record in metadata.line_items.iter() {
        if record.0.len() == 0 {
            return Err(QuickLendXError::InvalidDescription);
        }

        if record.1 <= 0 || record.2 < 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let expected_total = record.1.saturating_mul(record.2);
        if expected_total != record.3 {
            return Err(QuickLendXError::InvalidAmount);
        }

        computed_total = computed_total.saturating_add(record.3);
    }

    if computed_total != invoice_amount {
        return Err(QuickLendXError::InvoiceAmountInvalid);
    }

    Ok(())
}
