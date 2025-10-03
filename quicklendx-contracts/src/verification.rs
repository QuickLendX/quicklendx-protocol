use crate::bid::{BidStatus, BidStorage};
use crate::errors::QuickLendXError;
use crate::invoice::{Invoice, InvoiceMetadata};
use soroban_sdk::{contracttype, symbol_short, vec, Address, Env, String, Vec};

#[contracttype]
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
    pub status: BusinessVerificationStatus,
    pub verified_at: Option<u64>,
    pub verified_by: Option<Address>,
    pub kyc_data: String,
    pub investment_limit: i128,
    pub submitted_at: u64,
}

const MIN_BID_AMOUNT: i128 = 100;

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
                });
            }
        }

        if let Some(v) = verification {
            Self::store(env, &v);
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
        Self::store(env, verification);
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
            verification.status = BusinessVerificationStatus::Verified;
            verification.verified_at = Some(env.ledger().timestamp());
            verification.verified_by = Some(admin.clone());
            verification.investment_limit = investment_limit;
            InvestorVerificationStorage::update(env, &verification);
            Ok(verification)
        }
    }
}

pub fn reject_investor(
    env: &Env,
    admin: &Address,
    investor: &Address,
) -> Result<(), QuickLendXError> {
    admin.require_auth();
    let mut verification =
        InvestorVerificationStorage::get(env, investor).ok_or(QuickLendXError::KYCNotFound)?;

    verification.status = BusinessVerificationStatus::Rejected;
    verification.verified_at = Some(env.ledger().timestamp());
    verification.verified_by = Some(admin.clone());
    InvestorVerificationStorage::update(env, &verification);
    Ok(())
}

pub fn get_investor_verification(env: &Env, investor: &Address) -> Option<InvestorVerification> {
    InvestorVerificationStorage::get(env, investor)
}

pub fn is_investor_verified(env: &Env, investor: &Address) -> bool {
    if let Some(verification) = InvestorVerificationStorage::get(env, investor) {
        matches!(verification.status, BusinessVerificationStatus::Verified)
    } else {
        false
    }
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
