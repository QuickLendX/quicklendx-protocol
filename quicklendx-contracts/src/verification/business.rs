use super::{
    add_to_kyc_index, get_kyc_index, remove_from_kyc_index, KycIndexKind, KycStorageKey,
};
use crate::errors::QuickLendXError;
use crate::protocol_limits::{
    check_string_length, MAX_KYC_DATA_LENGTH, MAX_REJECTION_REASON_LENGTH,
};
use soroban_sdk::{contracttype, symbol_short, Address, Env, String, Vec};

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

pub struct BusinessVerificationStorage;

impl BusinessVerificationStorage {
    const VERIFIED_BUSINESSES_KEY: &'static str = "verified_businesses";
    const PENDING_BUSINESSES_KEY: &'static str = "pending_businesses";
    const REJECTED_BUSINESSES_KEY: &'static str = "rejected_businesses";
    const ADMIN_KEY: &'static str = "admin_address";
    const DELETED_BUSINESSES_KEY: &'static str = "deleted_businesses";

    /// Validates that a state transition is allowed according to KYC lifecycle rules
    ///
    /// Valid transitions:
    /// - None -> Pending (new submission)
    /// - Pending -> Verified (admin approval)
    /// - Pending -> Rejected (admin rejection)
    /// - Rejected -> Pending (resubmission after rejection)
    ///
    /// Invalid transitions:
    /// - Verified -> *any other state (verified is final)
    /// - Pending -> Pending (duplicate submission)
    /// - Rejected -> Rejected (duplicate rejection)
    /// - Rejected -> Verified (must go through Pending first)
    pub fn validate_state_transition(
        old_status: Option<BusinessVerificationStatus>,
        new_status: BusinessVerificationStatus,
    ) -> Result<(), QuickLendXError> {
        match (old_status, new_status) {
            // New submission (no previous status)
            (None, BusinessVerificationStatus::Pending) => Ok(()),

            // Pending -> Verified (admin approval)
            (Some(BusinessVerificationStatus::Pending), BusinessVerificationStatus::Verified) => {
                Ok(())
            }

            // Pending -> Rejected (admin rejection)
            (Some(BusinessVerificationStatus::Pending), BusinessVerificationStatus::Rejected) => {
                Ok(())
            }

            // Rejected -> Pending (resubmission after rejection)
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
            // If there was an old rejection reason and a new rejection reason is provided, they must match
            if let Some(old_reason) = &old_ver.rejection_reason {
                if let Some(new_reason) = new_rejection_reason {
                    if old_reason != new_reason {
                        return Err(QuickLendXError::InvalidKYCStatus); // Cannot change rejection reason
                    }
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
        let key = KycStorageKey::BusinessRecord(verification.business.clone());
        env.storage().persistent().set(&key, verification);
        crate::storage::extend_persistent_ttl(env, &key);

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
    let key = KycStorageKey::BusinessRecord(business.clone());
    let value = env.storage().persistent().get(&key);
    if value.is_some() {
        crate::storage::extend_persistent_ttl(env, &key);
    }
    value
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
        get_kyc_index(env, KycIndexKind::VerifiedBusinesses)
    }

    pub fn get_pending_businesses(env: &Env) -> Vec<Address> {
        get_kyc_index(env, KycIndexKind::PendingBusinesses)
    }

    pub fn get_rejected_businesses(env: &Env) -> Vec<Address> {
        get_kyc_index(env, KycIndexKind::RejectedBusinesses)
    }

    fn add_to_verified_businesses(env: &Env, business: &Address) {
        add_to_kyc_index(env, KycIndexKind::VerifiedBusinesses, business);
    }

    fn add_to_pending_businesses(env: &Env, business: &Address) {
        add_to_kyc_index(env, KycIndexKind::PendingBusinesses, business);
    }

    fn add_to_rejected_businesses(env: &Env, business: &Address) {
        add_to_kyc_index(env, KycIndexKind::RejectedBusinesses, business);
    }

    fn remove_from_verified_businesses(env: &Env, business: &Address) {
        remove_from_kyc_index(env, KycIndexKind::VerifiedBusinesses, business);
    }

    fn remove_from_pending_businesses(env: &Env, business: &Address) {
        remove_from_kyc_index(env, KycIndexKind::PendingBusinesses, business);
    }

    fn remove_from_rejected_businesses(env: &Env, business: &Address) {
        remove_from_kyc_index(env, KycIndexKind::RejectedBusinesses, business);
    }

    /// @deprecated Use `admin::AdminStorage::initialize()` or `admin::AdminStorage::set_admin()` instead
    /// This function is kept for backward compatibility with existing tests.
    /// Returns true if the business is marked as deleted.
    pub fn is_deleted(env: &Env, business: &Address) -> bool {
        let deleted = Self::get_deleted_businesses(env);
        deleted.iter().any(|addr| addr == *business)
    }

    /// Retrieve list of deleted businesses.
    pub fn get_deleted_businesses(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&Self::DELETED_BUSINESSES_KEY)
            .unwrap_or(vec![env])
    }

    fn add_to_deleted_businesses(env: &Env, business: &Address) {
        let mut deleted = Self::get_deleted_businesses(env);
        deleted.push_back(business.clone());
        env.storage()
            .instance()
            .set(&Self::DELETED_BUSINESSES_KEY, &deleted);
    }

    fn remove_from_deleted_businesses(env: &Env, business: &Address) {
        let deleted = Self::get_deleted_businesses(env);
        let mut new_deleted = vec![env];
        for addr in deleted.iter() {
            if addr != *business {
                new_deleted.push_back(addr);
            }
        }
        env.storage()
            .instance()
            .set(&Self::DELETED_BUSINESSES_KEY, &new_deleted);
    }

    /// Deletes a business: removes from any status list and marks as deleted.
    pub fn delete_business(env: &Env, business: &Address) -> Result<(), QuickLendXError> {
        // Remove from verified, pending, rejected lists if present
        if Self::is_business_verified(env, business) {
            Self::remove_from_verified_businesses(env, business);
        }
        if Self::is_business_pending(env, business) {
            Self::remove_from_pending_businesses(env, business);
        }
        if Self::is_business_rejected(env, business) {
            Self::remove_from_rejected_businesses(env, business);
        }
        // Add to deleted list
        if Self::is_deleted(env, business) {
            // Already deleted; no-op
            return Ok(());
        }
        Self::add_to_deleted_businesses(env, business);
        Ok(())
    }

    /// Restores a previously deleted business: removes from the deleted list and
    /// re-adds to the appropriate status list based on the existing verification record.
    ///
    /// # Errors
    /// - `BusinessNotVerified` if the business has no verification record.
    /// - `BusinessDeleted` if the business is not currently deleted (no-op).
    pub fn restore_business(env: &Env, business: &Address) -> Result<(), QuickLendXError> {
        if !Self::is_deleted(env, business) {
            return Err(QuickLendXError::BusinessDeleted);
        }
        Self::remove_from_deleted_businesses(env, business);
        // Re-add to the status list matching the existing verification record.
        if let Some(verification) = Self::get_verification(env, business) {
            match verification.status {
                BusinessVerificationStatus::Verified => {
                    Self::add_to_verified_businesses(env, business);
                }
                BusinessVerificationStatus::Pending => {
                    Self::add_to_pending_businesses(env, business);
                }
                BusinessVerificationStatus::Rejected => {
                    Self::add_to_rejected_businesses(env, business);
                }
            }
        }
        Ok(())
    }

    // Helper checks for status presence
    fn is_business_pending(env: &Env, business: &Address) -> bool {
        let pending = Self::get_pending_businesses(env);
        pending.iter().any(|addr| addr == *business)
    }
    fn is_business_rejected(env: &Env, business: &Address) -> bool {
        let rejected = Self::get_rejected_businesses(env);
        rejected.iter().any(|addr| addr == *business)
    }
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

    // Emit structured event and audit log entry
    let is_resubmit = matches!(old_status, Some(BusinessVerificationStatus::Rejected));
    crate::events::emit_kyc_submitted(env, business, is_resubmit);
    let old_status_str = match &old_status {
        Some(BusinessVerificationStatus::Pending) => Some(String::from_str(env, "Pending")),
        Some(BusinessVerificationStatus::Verified) => Some(String::from_str(env, "Verified")),
        Some(BusinessVerificationStatus::Rejected) => Some(String::from_str(env, "Rejected")),
        None => None,
    };
    crate::audit::log_kyc_operation(
        env,
        crate::audit::AuditOperation::KycSubmitted,
        business.clone(),
        old_status_str,
        Some(String::from_str(env, "Pending")),
        Some(String::from_str(
            env,
            if is_resubmit {
                "resubmission"
            } else {
                "new_submission"
            },
        )),
    );

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
    crate::events::emit_kyc_verified(env, business, admin);
    crate::audit::log_kyc_operation(
        env,
        crate::audit::AuditOperation::KycVerified,
        admin.clone(),
        Some(String::from_str(env, "Pending")),
        Some(String::from_str(env, "Verified")),
        Some(String::from_str(env, "business")),
    );
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
    crate::events::emit_kyc_rejected(env, business, admin, &reason);
    crate::audit::log_kyc_operation(
        env,
        crate::audit::AuditOperation::KycRejected,
        admin.clone(),
        Some(String::from_str(env, "Pending")),
        Some(String::from_str(env, "Rejected")),
        Some(reason),
    );
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

/// Enforce that a business KYC verification record has a valid (Verified) KYC status/tier.
///
/// Symmetric guard for business KYC tiers.
///
/// # Errors
/// - `KYCAlreadyPending` if the business KYC application is pending review
/// - `BusinessNotVerified` if the business KYC application is rejected or not verified
pub fn require_valid_business_kyc_tier(t: &BusinessVerification) -> Result<(), QuickLendXError> {
    match t.status {
        BusinessVerificationStatus::Verified => Ok(()),
        BusinessVerificationStatus::Pending => Err(QuickLendXError::KYCAlreadyPending),
        BusinessVerificationStatus::Rejected => Err(QuickLendXError::BusinessNotVerified),
    }
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
    if BusinessVerificationStorage::is_deleted(env, business) {
        return Err(QuickLendXError::BusinessDeleted);
    }
    match BusinessVerificationStorage::get_verification(env, business) {
        Some(v) => require_valid_business_kyc_tier(&v),
        None => Err(QuickLendXError::BusinessNotVerified),
    }
}
