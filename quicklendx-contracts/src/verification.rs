use soroban_sdk::{contracttype, symbol_short, vec, Address, Env, String, Symbol, Vec};
use crate::errors::QuickLendXError;

#[contracttype]
#[derive(Debug, PartialEq)]
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
pub struct InvestorVerification {
    pub investor: Address,
    pub status: BusinessVerificationStatus, // Reuse same enum
    pub verified_at: Option<u64>,
    pub verified_by: Option<Address>,
    pub kyc_data: String,
    pub investment_limit: i128, // Maximum investment amount
    pub submitted_at: u64,
    pub rejection_reason: Option<String>,
}

pub struct BusinessVerificationStorage;

impl BusinessVerificationStorage {
    const VERIFICATION_KEY: &'static str = "business_verification";
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

    pub fn set_admin(env: &Env, admin: &Address) {
        env.storage().instance().set(&Self::ADMIN_KEY, admin);
    }

    pub fn get_admin(env: &Env) -> Option<Address> {
        env.storage().instance().get(&Self::ADMIN_KEY)
    }

    pub fn is_admin(env: &Env, address: &Address) -> bool {
        if let Some(admin) = Self::get_admin(env) {
            admin == *address
        } else {
            false
        }
    }
}

pub struct InvestorVerificationStorage;

impl InvestorVerificationStorage {
    const INVESTOR_VERIFICATION_KEY: &'static str = "investor_verification";
    const VERIFIED_INVESTORS_KEY: &'static str = "verified_investors";
    const PENDING_INVESTORS_KEY: &'static str = "pending_investors";
    const REJECTED_INVESTORS_KEY: &'static str = "rejected_investors";

    pub fn store_verification(env: &Env, verification: &InvestorVerification) {
        env.storage()
            .instance()
            .set(&verification.investor, verification);

        // Add to status-specific lists
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

    pub fn get_verification(env: &Env, investor: &Address) -> Option<InvestorVerification> {
        env.storage().instance().get(investor)
    }

    pub fn update_verification(env: &Env, verification: &InvestorVerification) {
        let old_verification = Self::get_verification(env, &verification.investor);

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
        Self::store_verification(env, verification);
    }

    pub fn is_investor_verified(env: &Env, investor: &Address) -> bool {
        if let Some(verification) = Self::get_verification(env, investor) {
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
    business: &Address,
    amount: i128,
    _currency: &Address,
    due_date: u64,
    description: &String,
) -> Result<(), QuickLendXError> {
    // First check if business is verified
    require_business_verification(env, business)?;

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

// Investor verification functions
pub fn submit_investor_kyc_application(
    env: &Env,
    investor: &Address,
    kyc_data: String,
    investment_limit: i128,
) -> Result<(), QuickLendXError> {
    // Only the investor can submit their own KYC
    investor.require_auth();

    // Check if investor already has a verification record
    if let Some(existing_verification) =
        InvestorVerificationStorage::get_verification(env, investor)
    {
        match existing_verification.status {
            BusinessVerificationStatus::Pending => {
                return Err(QuickLendXError::InvestorKYCAlreadyPending);
            }
            BusinessVerificationStatus::Verified => {
                return Err(QuickLendXError::InvestorKYCAlreadyVerified);
            }
            BusinessVerificationStatus::Rejected => {
                // Allow resubmission after rejection
            }
        }
    }

    let verification = InvestorVerification {
        investor: investor.clone(),
        status: BusinessVerificationStatus::Pending,
        verified_at: None,
        verified_by: None,
        kyc_data,
        investment_limit,
        submitted_at: env.ledger().timestamp(),
        rejection_reason: None,
    };

    InvestorVerificationStorage::store_verification(env, &verification);
    emit_investor_kyc_submitted(env, investor);
    Ok(())
}

pub fn verify_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
    kyc_data: String,
    investment_limit: i128,
) -> Result<(), QuickLendXError> {
    // Verify admin authority
    admin.require_auth();
    if !BusinessVerificationStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    // Get existing verification or create new one
    let mut verification = InvestorVerificationStorage::get_verification(env, investor)
        .unwrap_or(InvestorVerification {
            investor: investor.clone(),
            status: BusinessVerificationStatus::Pending,
            verified_at: None,
            verified_by: None,
            kyc_data: kyc_data.clone(),
            investment_limit,
            submitted_at: env.ledger().timestamp(),
            rejection_reason: None,
        });

    verification.status = BusinessVerificationStatus::Verified;
    verification.verified_at = Some(env.ledger().timestamp());
    verification.verified_by = Some(admin.clone());
    verification.kyc_data = kyc_data;
    verification.investment_limit = investment_limit;
    verification.rejection_reason = None;

    InvestorVerificationStorage::update_verification(env, &verification);
    emit_investor_verified(env, investor, admin);
    Ok(())
}

pub fn reject_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
    rejection_reason: String,
) -> Result<(), QuickLendXError> {
    // Verify admin authority
    admin.require_auth();
    if !BusinessVerificationStorage::is_admin(env, admin) {
        return Err(QuickLendXError::NotAdmin);
    }

    let mut verification = InvestorVerificationStorage::get_verification(env, investor)
        .ok_or(QuickLendXError::InvestorKYCNotFound)?;

    verification.status = BusinessVerificationStatus::Rejected;
    verification.verified_at = None;
    verification.verified_by = Some(admin.clone());
    verification.rejection_reason = Some(rejection_reason);

    InvestorVerificationStorage::update_verification(env, &verification);
    emit_investor_rejected(env, investor, admin);
    Ok(())
}

pub fn get_investor_verification_status(
    env: &Env,
    investor: &Address,
) -> Result<BusinessVerificationStatus, QuickLendXError> {
    let verification = InvestorVerificationStorage::get_verification(env, investor)
        .ok_or(QuickLendXError::InvestorKYCNotFound)?;
    Ok(verification.status)
}

pub fn require_investor_verification(
    env: &Env,
    investor: &Address,
) -> Result<(), QuickLendXError> {
    if !InvestorVerificationStorage::is_investor_verified(env, investor) {
        return Err(QuickLendXError::InvestorNotVerified);
    }
    Ok(())
}

pub fn check_investment_limit(
    env: &Env,
    investor: &Address,
    amount: i128,
) -> Result<(), QuickLendXError> {
    let verification = InvestorVerificationStorage::get_verification(env, investor)
        .ok_or(QuickLendXError::InvestorKYCNotFound)?;
    
    if amount > verification.investment_limit {
        return Err(QuickLendXError::InvestmentLimitExceeded);
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
        (business.clone(), admin.clone(), env.ledger().timestamp()),
    );
}

fn emit_investor_kyc_submitted(env: &Env, investor: &Address) {
    env.events().publish(
        (symbol_short!("ikyc_sub"),),
        (investor.clone(), env.ledger().timestamp()),
    );
}

fn emit_investor_verified(env: &Env, investor: &Address, admin: &Address) {
    env.events().publish(
        (symbol_short!("inv_ver"),),
        (investor.clone(), admin.clone(), env.ledger().timestamp()),
    );
}

fn emit_investor_rejected(env: &Env, investor: &Address, admin: &Address) {
    env.events().publish(
        (symbol_short!("inv_rej"),),
        (investor.clone(), admin.clone(), env.ledger().timestamp()),
    );
}
