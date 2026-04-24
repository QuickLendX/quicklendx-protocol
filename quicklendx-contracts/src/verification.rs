use crate::bid::{BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::invoice::{Dispute, DisputeStatus, Invoice, InvoiceMetadata, InvoiceStatus};
use crate::protocol_limits::{
    check_string_length, ProtocolLimitsContract, MAX_ADDRESS_LENGTH, MAX_DESCRIPTION_LENGTH,
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
    MAX_KYC_DATA_LENGTH, MAX_NAME_LENGTH, MAX_NOTES_LENGTH, MAX_REJECTION_REASON_LENGTH,
    MAX_TAG_LENGTH, MAX_TAX_ID_LENGTH,
};
use soroban_sdk::{contracttype, symbol_short, vec, Address, Env, String, Vec};

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
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
#[derive(Clone, PartialEq, Debug)]
pub enum InvestorTier {
    Basic,
    Silver,
    Gold,
    Platinum,
    VIP,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
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

pub fn validate_risk_score(score: u32) -> Result<(), QuickLendXError> {
    if score > 100 {
        return Err(QuickLendXError::InvalidAmount);
    }
    Ok(())
}

pub struct BusinessVerificationStorage;

impl BusinessVerificationStorage {
    const VERIFIED_BUSINESSES_KEY: &'static str = "verified_businesses";
    const PENDING_BUSINESSES_KEY: &'static str = "pending_businesses";
    const REJECTED_BUSINESSES_KEY: &'static str = "rejected_businesses";
    const ADMIN_KEY: &'static str = "admin_address";

    /// Validates that a state transition is allowed according to KYC lifecycle rules
    ///
    /// Valid transitions:
    /// - None → Pending (new submission)
    /// - Pending → Verified (admin approval)
    /// - Pending → Rejected (admin rejection)
    /// - Rejected → Pending (resubmission after rejection)
    ///
    /// Invalid transitions:
    /// - Verified → *any other state (verified is final)
    /// - Pending → Pending (duplicate submission)
    /// - Rejected → Rejected (duplicate rejection)
    /// - Rejected → Verified (must go through Pending first)
    pub fn validate_state_transition(
        old_status: Option<BusinessVerificationStatus>,
        new_status: BusinessVerificationStatus,
    ) -> Result<(), QuickLendXError> {
        match (old_status, new_status) {
            // New submission (no previous status)
            (None, BusinessVerificationStatus::Pending) => Ok(()),

            // Pending → Verified (admin approval)
            (Some(BusinessVerificationStatus::Pending), BusinessVerificationStatus::Verified) => {
                Ok(())
            }

            // Pending → Rejected (admin rejection)
            (Some(BusinessVerificationStatus::Pending), BusinessVerificationStatus::Rejected) => {
                Ok(())
            }

            // Rejected → Pending (resubmission after rejection)
            (Some(BusinessVerificationStatus::Rejected), BusinessVerificationStatus::Pending) => {
                Ok(())
            }

            // Invalid transitions
            (Some(BusinessVerificationStatus::Verified), _) => {
                Err(QuickLendXError::InvalidKYCStatus) // Verified is final
            }
            (Some(BusinessVerificationStatus::Pending), BusinessVerificationStatus::Pending) => {
                Err(QuickLendXError::KYCAlreadyPending) // Duplicate submission
            }
            (Some(BusinessVerificationStatus::Rejected), BusinessVerificationStatus::Rejected) => {
                Err(QuickLendXError::InvalidKYCStatus) // Duplicate rejection
            }
            (Some(BusinessVerificationStatus::Rejected), BusinessVerificationStatus::Verified) => {
                Err(QuickLendXError::InvalidKYCStatus) // Must go through Pending first
            }
            (None, BusinessVerificationStatus::Verified) => {
                Err(QuickLendXError::InvalidKYCStatus) // Cannot be verified without submission
            }
            (None, BusinessVerificationStatus::Rejected) => {
                Err(QuickLendXError::InvalidKYCStatus) // Cannot be rejected without submission
            }
        }
    }

    /// Validates that rejection reason is immutable once set
    /// Once a business has been rejected with a reason, that reason cannot be changed
    pub fn validate_rejection_reason_immutability(
        old_verification: &Option<BusinessVerification>,
        new_rejection_reason: &Option<String>,
    ) -> Result<(), QuickLendXError> {
        if let Some(old_ver) = old_verification {
            // If there was an old rejection reason, the new one must match exactly
            if let Some(old_reason) = &old_ver.rejection_reason {
                if let Some(new_reason) = new_rejection_reason {
                    if old_reason != new_reason {
                        return Err(QuickLendXError::InvalidKYCStatus); // Cannot change rejection reason
                    }
                } else {
                    return Err(QuickLendXError::InvalidKYCStatus); // Cannot remove rejection reason
                }
            }
        }
        Ok(())
    }

    /// Verifies index consistency by checking that a business appears in exactly one status list
    pub fn verify_index_consistency(env: &Env, business: &Address) -> Result<(), QuickLendXError> {
        let verified = Self::get_verified_businesses(env);
        let pending = Self::get_pending_businesses(env);
        let rejected = Self::get_rejected_businesses(env);

        let in_verified = verified.iter().any(|addr| addr == *business);
        let in_pending = pending.iter().any(|addr| addr == *business);
        let in_rejected = rejected.iter().any(|addr| addr == *business);

        // Business should be in exactly one list
        let count = [in_verified, in_pending, in_rejected]
            .iter()
            .filter(|&&x| x)
            .count();
        if count != 1 {
            return Err(QuickLendXError::InvalidKYCStatus);
        }

        Ok(())
    }

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

    pub fn update_verification(
        env: &Env,
        verification: &BusinessVerification,
    ) -> Result<(), QuickLendXError> {
        let old_verification = Self::get_verification(env, &verification.business);
        let old_status = old_verification.as_ref().map(|v| v.status.clone());

        // Validate state transition
        Self::validate_state_transition(old_status.clone(), verification.status.clone())?;

        // Validate rejection reason immutability
        Self::validate_rejection_reason_immutability(
            &old_verification,
            &verification.rejection_reason,
        )?;

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

        // Verify index consistency after update
        Self::verify_index_consistency(env, &verification.business)?;

        Ok(())
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
    #[cfg(test)]
    const INVESTOR_HISTORY_KEY: &'static str = "investor_history";
    #[cfg(test)]
    const INVESTOR_ANALYTICS_KEY: &'static str = "investor_analytics";

    pub fn submit(env: &Env, investor: &Address, kyc_data: String) -> Result<(), QuickLendXError> {
        check_string_length(&kyc_data, MAX_KYC_DATA_LENGTH)?;
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

/// Normalizes a tag by trimming whitespace and converting to lowercase.
/// Enforces length limits of 1-50 characters.
pub fn normalize_tag(env: &Env, tag: &String) -> Result<String, QuickLendXError> {
    if tag.len() == 0 || tag.len() > MAX_TAG_LENGTH.saturating_mul(2) {
        return Err(QuickLendXError::InvalidTag);
    }

    let mut buf = [0u8; (MAX_TAG_LENGTH as usize) * 2];
    tag.copy_into_slice(&mut buf[..tag.len() as usize]);
    let raw_slice = &buf[..tag.len() as usize];

    let mut start = 0usize;
    let mut end = raw_slice.len();
    while start < end && raw_slice[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && raw_slice[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    if start == end {
        return Err(QuickLendXError::InvalidTag);
    }

    let normalized_len = end - start;
    if normalized_len > MAX_TAG_LENGTH as usize {
        return Err(QuickLendXError::InvalidTag);
    }

    let mut normalized_bytes = [0u8; MAX_TAG_LENGTH as usize];
    for (idx, &b) in raw_slice[start..end].iter().enumerate() {
        let lower = if b >= b'A' && b <= b'Z' { b + 32 } else { b };
        normalized_bytes[idx] = lower;
    }

    let normalized_str = String::from_str(
        env,
        core::str::from_utf8(&normalized_bytes[..normalized_len])
            .map_err(|_| QuickLendXError::InvalidTag)?,
    );

    if normalized_str.len() == 0 {
        return Err(QuickLendXError::InvalidTag);
    }
    Ok(normalized_str)
}

/// @notice Validate a bid against protocol rules and business constraints
/// @dev Enforces minimum bid amounts (both absolute and percentage-based),
///      invoice status checks, ownership validation, and investor capacity limits
/// @param env The contract environment
/// @param invoice The invoice being bid on
/// @param bid_amount The amount being bid
/// @param expected_return The expected return amount for the investor
/// @param investor The address of the bidding investor
/// @return Success if bid passes all validation rules
/// @error InvalidAmount if bid amount is below minimum or exceeds invoice amount
/// @error InvalidStatus if invoice is not in Verified state or is past due date
/// @error Unauthorized if business tries to bid on own invoice
/// @error OperationNotAllowed if investor already has an active bid on this invoice
/// @error InsufficientCapacity if bid exceeds investor's remaining investment capacity
pub fn validate_bid(
    env: &Env,
    invoice: &Invoice,
    bid_amount: i128,
    expected_return: i128,
    investor: &Address,
) -> Result<(), QuickLendXError> {
    // 1. Basic amount validation
    if bid_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    // 2. Invoice state and stale check
    if invoice.status != InvoiceStatus::Verified {
        return Err(QuickLendXError::InvalidStatus);
    }

    // Pre-maturity check: prevent bidding on invoices that have already reached their due date
    if env.ledger().timestamp() >= invoice.due_date {
        return Err(QuickLendXError::InvalidStatus);
    }

    // 3. Ownership check: Business cannot bid on its own invoice
    if &invoice.business == investor {
        return Err(QuickLendXError::Unauthorized);
    }

    // 4. Protocol limits and bid size validation
    let limits = ProtocolLimitsContract::get_protocol_limits(env.clone());

    // Calculate minimum bid amount using both absolute minimum and percentage-based minimum
    let percent_min = invoice
        .amount
        .saturating_mul(limits.min_bid_bps as i128)
        .saturating_div(10_000);
    let effective_min_bid = if percent_min > limits.min_bid_amount {
        percent_min
    } else {
        limits.min_bid_amount
    };

    if bid_amount < effective_min_bid {
        return Err(QuickLendXError::InvalidAmount);
    }

    if bid_amount > invoice.amount {
        return Err(QuickLendXError::InvoiceAmountInvalid);
    }

    // Expected return must exceed the original bid to avoid negative payoff.
    if expected_return <= bid_amount {
        return Err(QuickLendXError::InvalidAmount);
    }

    // 5. Investor Eligibility and Capacity
    // This checks both verification status AND individual/risk-based investment limits
    validate_investor_investment(env, investor, bid_amount)?;

    // 6. Existing Bid Protection
    BidStorage::cleanup_expired_bids(env, &invoice.id);
    let existing_bids = BidStorage::get_bids_for_invoice(env, &invoice.id);
    for bid_id in existing_bids.iter() {
        if let Some(existing_bid) = BidStorage::get_bid(env, &bid_id) {
            // Prevent multiple active bids from the same investor on one invoice
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
    check_string_length(&kyc_data, MAX_KYC_DATA_LENGTH)?;
    // Only the business can submit their own KYC
    business.require_auth();

    // Get existing verification record
    let existing_verification = BusinessVerificationStorage::get_verification(env, business);
    let old_status = existing_verification.as_ref().map(|v| v.status.clone());

    // Validate state transition to Pending
    BusinessVerificationStorage::validate_state_transition(
        old_status.clone(),
        BusinessVerificationStatus::Pending,
    )?;

    let verification = BusinessVerification {
        business: business.clone(),
        status: BusinessVerificationStatus::Pending,
        verified_at: None,
        verified_by: None,
        kyc_data,
        submitted_at: env.ledger().timestamp(),
        rejection_reason: None, // Clear rejection reason on resubmission
    };

    BusinessVerificationStorage::update_verification(env, &verification)?;

    // Emit appropriate event based on whether this is a resubmission
    if matches!(old_status, Some(BusinessVerificationStatus::Rejected)) {
        emit_kyc_resubmitted(env, business);
    } else {
        emit_kyc_submitted(env, business);
    }

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

    // Validate state transition to Verified
    BusinessVerificationStorage::validate_state_transition(
        Some(verification.status.clone()),
        BusinessVerificationStatus::Verified,
    )?;

    verification.status = BusinessVerificationStatus::Verified;
    verification.verified_at = Some(env.ledger().timestamp());
    verification.verified_by = Some(admin.clone());
    // Clear rejection reason when verified
    verification.rejection_reason = None;

    BusinessVerificationStorage::update_verification(env, &verification)?;
    emit_business_verified(env, business, admin);
    Ok(())
}

/// Reject a pending business KYC record with an auditable reason.
///
/// # Errors
/// - `NotAdmin` if `admin` is not a contract admin
/// - `KYCNotFound` if the business has no KYC record
/// - `InvalidKYCStatus` if the business is not currently `Pending`
/// - `InvalidDescription` if `reason` exceeds `MAX_REJECTION_REASON_LENGTH`
pub fn reject_business(
    env: &Env,
    admin: &Address,
    business: &Address,
    reason: String,
) -> Result<(), QuickLendXError> {
    check_string_length(&reason, MAX_REJECTION_REASON_LENGTH)?;
    // Only admin can reject businesses
    admin.require_auth();
    if !BusinessVerificationStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    let mut verification = BusinessVerificationStorage::get_verification(env, business)
        .ok_or(QuickLendXError::KYCNotFound)?;

    // Validate state transition to Rejected
    BusinessVerificationStorage::validate_state_transition(
        Some(verification.status.clone()),
        BusinessVerificationStatus::Rejected,
    )?;

    verification.status = BusinessVerificationStatus::Rejected;
    verification.rejection_reason = Some(reason.clone());

    BusinessVerificationStorage::update_verification(env, &verification)?;
    emit_business_rejected(env, business, admin, &reason);
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

/// Enforce that a business is not in KYC-pending state before allowing a sensitive operation.
///
/// Pending businesses have submitted KYC but have not yet been approved or rejected.
/// They must not be allowed to perform privileged actions (e.g. upload invoices, cancel
/// invoices, accept bids) until their identity has been confirmed by an admin.
///
/// # Errors
/// - `KYCAlreadyPending` if the business has a pending KYC application
/// - `BusinessNotVerified` if the business has no KYC record or is rejected
pub fn require_business_not_pending(env: &Env, business: &Address) -> Result<(), QuickLendXError> {
    match BusinessVerificationStorage::get_verification(env, business) {
        Some(v) => match v.status {
            BusinessVerificationStatus::Pending => Err(QuickLendXError::KYCAlreadyPending),
            BusinessVerificationStatus::Verified => Ok(()),
            BusinessVerificationStatus::Rejected => Err(QuickLendXError::BusinessNotVerified),
        },
        None => Err(QuickLendXError::BusinessNotVerified),
    }
}

/// Enforce that an investor is not in KYC-pending state before allowing a sensitive operation.
///
/// Pending investors have submitted KYC but have not yet been approved or rejected.
/// They must not be allowed to place bids, withdraw bids, or perform any investment
/// action until their identity has been confirmed by an admin.
///
/// # Errors
/// - `KYCAlreadyPending` if the investor has a pending KYC application
/// - `BusinessNotVerified` if the investor has no KYC record or is rejected
pub fn require_investor_not_pending(env: &Env, investor: &Address) -> Result<(), QuickLendXError> {
    match InvestorVerificationStorage::get(env, investor) {
        Some(v) => match v.status {
            BusinessVerificationStatus::Pending => Err(QuickLendXError::KYCAlreadyPending),
            BusinessVerificationStatus::Verified => Ok(()),
            BusinessVerificationStatus::Rejected => Err(QuickLendXError::BusinessNotVerified),
        },
        None => Err(QuickLendXError::BusinessNotVerified),
    }
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

    // Validate due date bounds using protocol limits (Default 365 days)
    let limits = crate::protocol_limits::ProtocolLimitsContract::get_protocol_limits(env.clone());
    let max_horizon = (limits.max_due_date_days as u64).saturating_mul(86400);
    let max_due_date = current_timestamp.saturating_add(max_horizon);

    if due_date > max_due_date {
        return Err(QuickLendXError::InvoiceDueDateInvalid); // Code 1008
    }

    check_string_length(description, MAX_DESCRIPTION_LENGTH)?;
    if description.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }
    Ok(())
}

// Enhanced event emission functions for comprehensive audit trail
fn emit_kyc_submitted(env: &Env, business: &Address) {
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("kyc_sub"),),
        (
            business.clone(),
            env.ledger().timestamp(),
            String::from_str(env, "submitted"),
        ),
    );
}

fn emit_business_verified(env: &Env, business: &Address, admin: &Address) {
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("bus_ver"),),
        (
            business.clone(),
            admin.clone(),
            env.ledger().timestamp(),
            String::from_str(env, "verified"),
        ),
    );
}

fn emit_business_rejected(env: &Env, business: &Address, admin: &Address, reason: &String) {
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("bus_rej"),),
        (
            business.clone(),
            admin.clone(),
            env.ledger().timestamp(),
            reason.clone(),
        ),
    );
}

fn emit_kyc_resubmitted(env: &Env, business: &Address) {
    #[allow(deprecated)]
    env.events().publish(
        (symbol_short!("kyc_resub"),),
        (
            business.clone(),
            env.ledger().timestamp(),
            String::from_str(env, "resubmitted"),
        ),
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

/// Validate invoice tags.
///
/// Each tag is normalized (trimmed, ASCII-lowercased) before validation so that
/// length checks and duplicate detection operate on the canonical stored form.
///
/// # Rules enforced
/// - Tag count ≤ 10.
/// - Each normalized tag must be 1–50 bytes.
/// - No two tags may normalize to the same value (e.g. "Tech" and "tech" are duplicates).
///
/// # Errors
/// - `TagLimitExceeded` (1801): more than 10 tags supplied.
/// - `InvalidTag` (1800): a tag is empty/too long after normalization, or is a duplicate.
pub fn validate_invoice_tags(env: &Env, tags: &Vec<String>) -> Result<(), QuickLendXError> {
    if tags.len() > 10 {
        return Err(QuickLendXError::TagLimitExceeded);
    }

    let mut seen: Vec<String> = Vec::new(env);
    for tag in tags.iter() {
        let normalized = normalize_tag(env, &tag)?;

        if normalized.len() == 0 || normalized.len() > 50 {
            return Err(QuickLendXError::InvalidTag);
        }

        // Reject duplicates after normalization.
        for s in seen.iter() {
            if s == normalized {
                return Err(QuickLendXError::InvalidTag);
            }
        }
        seen.push_back(normalized);
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
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

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
            validate_risk_score(risk_score)?;
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

/// Reject a pending investor KYC record with an auditable reason.
///
/// # Errors
/// - `NotAdmin` if `admin` is not a contract admin
/// - `KYCNotFound` if the investor has no KYC record
/// - `InvalidKYCStatus` if the investor is not currently `Pending`
/// - `InvalidDescription` if `reason` exceeds `MAX_REJECTION_REASON_LENGTH`
pub fn reject_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
    reason: String,
) -> Result<(), QuickLendXError> {
    check_string_length(&reason, MAX_REJECTION_REASON_LENGTH)?;
    admin.require_auth();
    if !crate::admin::AdminStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }
    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;
    if !matches!(verification.status, BusinessVerificationStatus::Pending) {
        return Err(QuickLendXError::InvalidKYCStatus);
    }

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
    validate_risk_score(risk_score)?;

    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        let total_invested = verification.total_invested;
        let successful_investments = verification.successful_investments;

        match risk_score {
            0..=10 if total_invested > 5_000_000 && successful_investments > 50 => {
                return Ok(InvestorTier::VIP);
            }
            11..=20 if total_invested > 1_000_000 && successful_investments > 20 => {
                return Ok(InvestorTier::Platinum);
            }
            21..=40 if total_invested > 100_000 && successful_investments > 10 => {
                return Ok(InvestorTier::Gold);
            }
            41..=60 if total_invested > 10_000 && successful_investments > 3 => {
                return Ok(InvestorTier::Silver);
            }
            _ => {}
        }
    }

    Ok(InvestorTier::Basic)
}

/// Determine risk level based on risk score
pub fn determine_risk_level(risk_score: u32) -> InvestorRiskLevel {
    match risk_score {
        0..=25 => InvestorRiskLevel::Low,
        26..=50 => InvestorRiskLevel::Medium,
        51..=75 => InvestorRiskLevel::High,
        76..=100 => InvestorRiskLevel::VeryHigh,
        _ => InvestorRiskLevel::VeryHigh, // fallback safety
    }
}

/// Calculate investment limit based on tier and risk level
pub fn calculate_investment_limit(
    tier: &InvestorTier,
    risk_level: &InvestorRiskLevel,
    base_limit: i128,
) -> i128 {
    let tier_multiplier = get_tier_multiplier(tier);
    let risk_multiplier = get_risk_multiplier(risk_level);

    let calculated_limit = base_limit.max(0).saturating_mul(tier_multiplier);
    calculated_limit
        .saturating_mul(risk_multiplier)
        .saturating_div(100)
}

fn get_tier_multiplier(tier: &InvestorTier) -> i128 {
    match tier {
        InvestorTier::VIP => 10,
        InvestorTier::Platinum => 5,
        InvestorTier::Gold => 3,
        InvestorTier::Silver => 2,
        InvestorTier::Basic => 1,
    }
}

fn get_risk_multiplier(risk_level: &InvestorRiskLevel) -> i128 {
    match risk_level {
        InvestorRiskLevel::Low => 100,     // 100% of calculated limit
        InvestorRiskLevel::Medium => 75,   // 75% of calculated limit
        InvestorRiskLevel::High => 50,     // 50% of calculated limit
        InvestorRiskLevel::VeryHigh => 25, // 25% of calculated limit
    }
}

fn recover_base_limit_from_current_limit(
    current_limit: i128,
    tier: &InvestorTier,
    risk_level: &InvestorRiskLevel,
) -> i128 {
    let tier_multiplier = get_tier_multiplier(tier);
    let risk_multiplier = get_risk_multiplier(risk_level);
    let combined_multiplier = tier_multiplier.saturating_mul(risk_multiplier);
    if combined_multiplier <= 0 {
        return current_limit.max(0);
    }

    // Ceiling division avoids gradually shrinking the recovered base from integer truncation.
    current_limit
        .max(0)
        .saturating_mul(100)
        .saturating_add(combined_multiplier - 1)
        .saturating_div(combined_multiplier)
}

/// Update investor analytics after an investment
pub fn update_investor_analytics(
    env: &Env,
    investor: &Address,
    investment_amount: i128,
    is_successful: bool,
) -> Result<(), QuickLendXError> {
    if investment_amount <= 0 {
        return Err(QuickLendXError::InvalidAmount);
    }

    if let Some(mut verification) = InvestorVerificationStorage::get(env, investor) {
        let prior_base_limit = recover_base_limit_from_current_limit(
            verification.investment_limit,
            &verification.tier,
            &verification.risk_level,
        );

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

        // Preserve the investor's approved baseline and only re-derive the
        // dynamic limit using the updated tier/risk profile.
        let base_limit = prior_base_limit.max(1);
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
        // 1. Verification status check
        if !matches!(verification.status, BusinessVerificationStatus::Verified) {
            return Err(QuickLendXError::BusinessNotVerified);
        }

        // 2. Aggregate Limit Check
        // Ensure that (new bid + existing active bids + total funded investments) fits within the limit
        let active_bid_exposure = BidStorage::get_active_bid_amount_sum_for_investor(env, investor);
        let total_risk_exposure = active_bid_exposure
            .saturating_add(verification.total_invested)
            .saturating_add(investment_amount);

        if total_risk_exposure > verification.investment_limit {
            return Err(QuickLendXError::InvalidAmount);
        }

        // 3. Risk-Based Tiered Checks
        // Further constraints based on the specific risk level assigned by Admin
        match verification.risk_level {
            InvestorRiskLevel::VeryHigh => {
                // Individual bid amount caps for high-risk profiles
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
    check_string_length(&metadata.customer_name, MAX_NAME_LENGTH)?;
    if metadata.customer_name.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    check_string_length(&metadata.customer_address, MAX_ADDRESS_LENGTH)?;
    if metadata.customer_address.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    check_string_length(&metadata.tax_id, MAX_TAX_ID_LENGTH)?;
    if metadata.tax_id.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    check_string_length(&metadata.notes, MAX_NOTES_LENGTH)?;

    if metadata.line_items.len() == 0 {
        return Err(QuickLendXError::InvalidDescription);
    }

    let mut computed_total = 0i128;
    for record in metadata.line_items.iter() {
        check_string_length(&record.0, MAX_DESCRIPTION_LENGTH)?;
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

// ============================================================================
// Dispute Evidence & Reason Validation
// ============================================================================

/// @notice Validate dispute reason string.
/// @dev Rejects empty strings and strings exceeding MAX_DISPUTE_REASON_LENGTH (1000 chars).
///      Prevents abusive on-chain storage growth from oversized payloads.
/// @param reason The dispute reason to validate.
/// @return Ok(()) if valid, Err(InvalidDisputeReason) otherwise.
pub fn validate_dispute_reason(reason: &String) -> Result<(), QuickLendXError> {
    if reason.len() == 0 {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    if reason.len() > MAX_DISPUTE_REASON_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    Ok(())
}

/// @notice Validate dispute evidence string.
/// @dev Rejects empty strings and strings exceeding MAX_DISPUTE_EVIDENCE_LENGTH (2000 chars).
///      Evidence is required to prevent frivolous disputes and bounded to limit storage.
/// @param evidence The dispute evidence to validate.
/// @return Ok(()) if valid, Err(InvalidDisputeEvidence) otherwise.
pub fn validate_dispute_evidence(evidence: &String) -> Result<(), QuickLendXError> {
    if evidence.len() == 0 {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    if evidence.len() > MAX_DISPUTE_EVIDENCE_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    Ok(())
}

/// @notice Validate dispute resolution string.
/// @dev Rejects empty strings and strings exceeding MAX_DISPUTE_RESOLUTION_LENGTH (2000 chars).
/// @param resolution The resolution text to validate.
/// @return Ok(()) if valid, Err(InvalidDisputeReason) otherwise.
pub fn validate_dispute_resolution(resolution: &String) -> Result<(), QuickLendXError> {
    if resolution.len() == 0 {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    if resolution.len() > MAX_DISPUTE_RESOLUTION_LENGTH {
        return Err(QuickLendXError::InvalidDisputeReason);
    }
    Ok(())
}

/// @notice Validate that an invoice is eligible for dispute creation.
/// @dev Only invoices in Pending, Verified, Funded, or Paid status can be disputed.
///      The creator must be the business owner or the investor on the invoice.
///      Only one dispute per invoice is allowed.
/// @param invoice The invoice to check.
/// @param creator The address attempting to create the dispute.
/// @return Ok(()) if eligible, Err with appropriate error otherwise.
pub fn validate_dispute_eligibility(
    invoice: &Invoice,
    creator: &Address,
) -> Result<(), QuickLendXError> {
    // Check invoice status allows disputes
    match invoice.status {
        InvoiceStatus::Pending
        | InvoiceStatus::Verified
        | InvoiceStatus::Funded
        | InvoiceStatus::Paid => {}
        _ => return Err(QuickLendXError::InvoiceNotAvailableForFunding),
    }

    // Check creator is authorized (business or investor)
    let is_authorized = *creator == invoice.business
        || invoice
            .investor
            .as_ref()
            .map_or(false, |inv| *creator == *inv);
    if !is_authorized {
        return Err(QuickLendXError::DisputeNotAuthorized);
    }

    // Check no existing dispute
    if invoice.dispute_status != DisputeStatus::None {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    Ok(())
}
