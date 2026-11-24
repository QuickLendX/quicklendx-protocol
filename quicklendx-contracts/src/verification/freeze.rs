use super::BusinessVerificationStorage;
use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use soroban_sdk::{Address, Env};

/// Enforce that a business is active (not deleted/frozen) before performing an operation.
///
/// A deleted/frozen business must not be allowed to mutate any outstanding invoices.
/// This is a defence-in-depth check that complements the KYC status guards.
///
/// # Errors
/// - `BusinessDeleted` if the business has been deleted/frozen.
pub fn require_business_active(env: &Env, business: &Address) -> Result<(), QuickLendXError> {
    if BusinessVerificationStorage::is_deleted(env, business) {
        return Err(QuickLendXError::BusinessDeleted);
    }
    Ok(())
}

/// Enforce that an investor is not frozen before performing an operation.
///
/// A frozen investor must not be allowed to place bids, withdraw bids, or
/// perform any investment action until unfrozen by an admin.
///
/// # Errors
/// - `InvestorFrozen` if the investor has a freeze record.
pub fn require_investor_not_frozen(env: &Env, investor: &Address) -> Result<(), QuickLendXError> {
    if InvoiceStorage::get_investor_freeze_info(env, investor).is_some() {
        return Err(QuickLendXError::InvestorFrozen);
    }
    Ok(())
}

/// Regulatory compliance gate, reserved for future jurisdiction/sanctions-list
/// checks (see `docs/contracts/currency-whitelist.md` "Regulatory Compliance").
///
/// No such checks exist yet: this is intentionally a no-op by default and
/// unconditionally returns `Ok(())` for any address and any ledger/storage
/// state. Call sites can be wired in ahead of the actual regulatory logic
/// landing so they don't need to change again once it does.
pub fn require_regulatory_ok(_env: &Env, _address: &Address) -> Result<(), QuickLendXError> {
    Ok(())
}
