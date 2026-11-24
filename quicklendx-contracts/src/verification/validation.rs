use super::{
    require_business_active, require_business_not_pending, require_investor_not_frozen,
    require_investor_not_pending, validate_investor_investment,
};
use crate::bid::BidStorage;
use crate::errors::QuickLendXError;
use crate::protocol_limits::{
    check_string_length, ProtocolLimitsContract, MAX_ADDRESS_LENGTH, MAX_DESCRIPTION_LENGTH,
    MAX_DISPUTE_EVIDENCE_LENGTH, MAX_DISPUTE_REASON_LENGTH, MAX_DISPUTE_RESOLUTION_LENGTH,
    MAX_INVOICE_AMOUNT, MAX_NAME_LENGTH, MAX_NOTES_LENGTH, MAX_TAG_LENGTH, MAX_TAX_ID_LENGTH,
};
use crate::types::{BidStatus, DisputeStatus, Invoice, InvoiceMetadata, InvoiceStatus};
use soroban_sdk::{vec, Address, Bytes, Env, String, Vec};

/// Maximum normalized tags allowed on an invoice.
pub const MAX_INVOICE_TAG_COUNT: u32 = 10;
/// Maximum line items allowed in structured invoice metadata.
pub const MAX_METADATA_LINE_ITEMS: u32 = 100;

/// Normalizes a tag by trimming whitespace and converting to lowercase.
/// Enforces length limits of 1-50 characters.
pub fn normalize_tag(env: &Env, tag: &String) -> Result<String, QuickLendXError> {
    if tag.is_empty() || tag.len() > MAX_TAG_LENGTH.saturating_mul(2) {
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
        let lower = if b.is_ascii_uppercase() { b + 32 } else { b };
        normalized_bytes[idx] = lower;
    }

    let normalized_str = String::from_str(
        env,
        core::str::from_utf8(&normalized_bytes[..normalized_len])
            .map_err(|_| QuickLendXError::InvalidTag)?,
    );

    if normalized_str.is_empty() {
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
/// @error PerInvestorPositionCapExceeded if bid exceeds invoice per_investor_position_cap
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

    // Per-invoice whale defence: reject bids above the configured position cap.
    // Missing / None storage means uncapped (invoice.amount remains the ceiling).
    if let Some(cap) = InvoiceStorage::get_per_investor_position_cap(env, &invoice.id) {
        if bid_amount > cap {
            return Err(QuickLendXError::PerInvestorPositionCapExceeded);
        }
    }

    // Expected return must exceed the original bid to avoid negative payoff.
    if expected_return <= bid_amount {
        return Err(QuickLendXError::InvalidAmount);
    }

    // Expected return must fit safely in i128 arithmetic.
    if expected_return > MAX_INVOICE_AMOUNT {
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

    if amount <= 0 || amount > crate::protocol_limits::MAX_INVOICE_AMOUNT {
        return Err(QuickLendXError::InvalidAmount);
    }
    let current_timestamp = env.ledger().timestamp();
    if due_date <= current_timestamp {
        return Err(QuickLendXError::InvoiceDueDateInvalid);
    }

    // Validate due date bounds using protocol limits (Default 365 days)
    let limits = crate::protocol_limits::ProtocolLimitsContract::get_protocol_limits(env.clone());

    if amount < limits.min_invoice_amount {
        return Err(QuickLendXError::InvalidAmount);
    }

    let max_horizon = limits.max_due_date_days.saturating_mul(86400);
    let max_due_date = current_timestamp.saturating_add(max_horizon);

    if due_date > max_due_date {
        return Err(QuickLendXError::InvoiceDueDateInvalid); // Code 1008
    }

    check_string_length(description, MAX_DESCRIPTION_LENGTH)?;
    if description.is_empty() {
        return Err(QuickLendXError::InvalidDescription);
    }
    Ok(())
}

// Enhanced event emission functions for comprehensive audit trail
// (All KYC events are now emitted via crate::events::* for schema stability)

/// Validate invoice category
pub fn validate_invoice_category(
    category: &crate::types::InvoiceCategory,
) -> Result<(), QuickLendXError> {
    // All categories are valid as they are defined in the enum
    // This function can be extended to add additional validation logic if needed
    match category {
        crate::types::InvoiceCategory::Services => Ok(()),
        crate::types::InvoiceCategory::Goods => Ok(()),
        crate::types::InvoiceCategory::Consulting => Ok(()),
        crate::types::InvoiceCategory::Logistics => Ok(()),
        crate::types::InvoiceCategory::Products => Ok(()),
        crate::types::InvoiceCategory::Manufacturing => Ok(()),
        crate::types::InvoiceCategory::Technology => Ok(()),
        crate::types::InvoiceCategory::Healthcare => Ok(()),
        crate::types::InvoiceCategory::Other => Ok(()),
    }
}

/// Reject unknown or reserved invoice categories.
///
/// This is a tighter validation than [`validate_invoice_category`]: the
/// catch-all `InvoiceCategory::Other` is treated as *reserved* and rejected
/// because it is too generic for new invoices.  Requiring a specific category
/// improves data quality, makes analytics more useful, and gives investors a
/// clearer picture of the invoice they are funding.
///
/// As the protocol evolves, additional enum variants may be added that should
/// also be rejected (e.g. a placeholder for a future regulatory regime, or a
/// deprecated alias).  This function is the single point where those
/// rejections are enforced, following the `require_*` naming convention used
/// throughout the contract (`require_business_verification`,
/// `require_regulatory_ok`, etc.).
///
/// # Threat mitigated
///
/// Without an explicit category-allowlist separate from the enum definition, a
/// business could default to `Other` for every invoice, defeating the
/// categorisation that investors and the protocol rely on for risk assessment.
/// By treating `Other` as reserved, we force callers to select a meaningful
/// category or add a new variant to the enum if none of the existing options
/// truly fits.
///
/// # Errors
///
/// Returns `InvalidTag` for the reserved `Other` category.
pub fn require_valid_invoice_category(
    category: &crate::types::InvoiceCategory,
) -> Result<(), QuickLendXError> {
    // "Other" is reserved and not accepted for new invoices.
    if matches!(category, crate::types::InvoiceCategory::Other) {
        return Err(QuickLendXError::InvalidTag);
    }
    validate_invoice_category(category)
}

/// Validate invoice tags.
///
/// Each tag is normalized (trimmed, ASCII-lowercased) before validation so that
/// length checks and duplicate detection operate on the canonical stored form.
///
/// # Rules enforced
/// - Tag count - 10.
/// - Each normalized tag must be 1-50 bytes.
/// - No two tags may normalize to the same value (e.g. "Tech" and "tech" are duplicates).
///
/// # Errors
/// - `TagLimitExceeded` (1801): more than 10 tags supplied.
/// - `InvalidTag` (1800): a tag is empty/too long after normalization, or is a duplicate.
pub fn validate_invoice_tags(env: &Env, tags: &Vec<String>) -> Result<(), QuickLendXError> {
    if tags.len() > MAX_INVOICE_TAG_COUNT {
        return Err(QuickLendXError::TagLimitExceeded);
    }

    let mut seen: Vec<String> = Vec::new(env);
    for tag in tags.iter() {
        let normalized = normalize_tag(env, &tag)?;

        if normalized.is_empty() || normalized.len() > 50 {
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

/// Validate structured invoice metadata against the invoice amount
pub fn validate_invoice_metadata(
    metadata: &InvoiceMetadata,
    invoice_amount: i128,
) -> Result<(), QuickLendXError> {
    check_string_length(&metadata.customer_name, MAX_NAME_LENGTH)?;
    if metadata.customer_name.is_empty() {
        return Err(QuickLendXError::InvalidDescription);
    }

    check_string_length(&metadata.customer_address, MAX_ADDRESS_LENGTH)?;
    if metadata.customer_address.is_empty() {
        return Err(QuickLendXError::InvalidDescription);
    }

    check_string_length(&metadata.tax_id, MAX_TAX_ID_LENGTH)?;
    if metadata.tax_id.is_empty() {
        return Err(QuickLendXError::InvalidDescription);
    }

    check_string_length(&metadata.notes, MAX_NOTES_LENGTH)?;

    if metadata.line_items.is_empty() {
        return Err(QuickLendXError::InvalidDescription);
    }
    if metadata.line_items.len() > MAX_METADATA_LINE_ITEMS {
        return Err(QuickLendXError::InvalidDescription);
    }

    let mut computed_total = 0i128;
    for record in metadata.line_items.iter() {
        check_string_length(&record.0, MAX_DESCRIPTION_LENGTH)?;
        if record.0.is_empty() {
            return Err(QuickLendXError::InvalidDescription);
        }

        if record.1 <= 0 || record.2 < 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Enforce checked multiplication to prevent overflow
        let expected_total = (record.1 as i128)
            .checked_mul(record.2)
            .ok_or(QuickLendXError::InvalidAmount)?;
        if expected_total != record.3 {
            return Err(QuickLendXError::InvalidAmount);
        }

        // Enforce checked addition to prevent overflow
        computed_total = computed_total
            .checked_add(record.3)
            .ok_or(QuickLendXError::InvalidAmount)?;
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
    if reason.is_empty() {
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
    if evidence.is_empty() {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    if evidence.len() > MAX_DISPUTE_EVIDENCE_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    Ok(())
}

/// Required length for an evidence hash (`BytesN<32>` / SHA-256 digest size).
pub const EVIDENCE_HASH_LENGTH: u32 = 32;

/// @notice Validate evidence hash format (32-byte BytesN required).
/// @dev Evidence hashes must be exactly [`EVIDENCE_HASH_LENGTH`] bytes so callers can
///      safely convert the payload to `BytesN<32>` (SHA-256 and similar digests).
/// @param evidence_hash The raw evidence hash bytes to validate.
/// @return Ok(()) if `evidence_hash.len() == 32`, Err(InvalidDisputeEvidence) otherwise.
pub fn validate_evidence_hash(evidence_hash: &Bytes) -> Result<(), QuickLendXError> {
    if evidence_hash.len() != EVIDENCE_HASH_LENGTH {
        return Err(QuickLendXError::InvalidDisputeEvidence);
    }
    Ok(())
}

/// @notice Validate transaction hash format (64-character hex string required).
/// @dev Used to validate `transaction_id` during settlement partial payments to prevent
///      replay bypasses or storage bloat via malformed/empty nonces.
/// @param hash The transaction hash string to validate.
/// @return Ok(()) if valid 64-char hex, Err(InvalidTransactionHash) otherwise.
pub fn validate_transaction_hash(
    env: &soroban_sdk::Env,
    hash: &soroban_sdk::String,
) -> Result<(), crate::errors::QuickLendXError> {
    if hash.len() != 64 {
        return Err(crate::errors::QuickLendXError::InvalidTransactionHash);
    }

    let bytes = hash.to_bytes();
    for i in 0..bytes.len() {
        let b = bytes.get(i).unwrap();
        match b {
            b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {}
            _ => return Err(crate::errors::QuickLendXError::InvalidTransactionHash),
        }
    }

    Ok(())
}

/// @notice Validate dispute resolution string.
/// @dev Rejects empty strings and strings exceeding MAX_DISPUTE_RESOLUTION_LENGTH (2000 chars).
/// @param resolution The resolution text to validate.
/// @return Ok(()) if valid, Err(InvalidDisputeReason) otherwise.
pub fn validate_dispute_resolution(resolution: &String) -> Result<(), QuickLendXError> {
    if resolution.is_empty() {
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
            .is_some_and(|inv| *creator == *inv);
    if !is_authorized {
        return Err(QuickLendXError::DisputeNotAuthorized);
    }

    // Check no existing dispute
    if invoice.dispute_status != DisputeStatus::None {
        return Err(QuickLendXError::DisputeAlreadyExists);
    }

    Ok(())
}

#[cfg(test)]
mod test_invoice_category_helper {
    use super::*;
    use crate::types::InvoiceCategory;
    use proptest::prelude::*;
    use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

    #[test]
    fn test_known_categories_valid() {
        let known = [
            InvoiceCategory::Services,
            InvoiceCategory::Goods,
            InvoiceCategory::Consulting,
            InvoiceCategory::Logistics,
            InvoiceCategory::Products,
            InvoiceCategory::Manufacturing,
            InvoiceCategory::Technology,
            InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ];
        for cat in known {
            assert_eq!(validate_invoice_category(&cat), Ok(()));
        }
    }

    #[test]
    fn test_reserved_category_invalid() {
        let env = Env::default();
        // 9 is the first reserved/undefined discriminant
        let val: Val = 9u32.into_val(&env);
        let cat_res: Result<InvoiceCategory, _> = InvoiceCategory::try_from_val(&env, &val);
        assert!(
            cat_res.is_err(),
            "Reserved category 9 should fail deserialization"
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn test_arbitrary_category_invalid(i in 9u32..=u32::MAX) {
            let env = Env::default();
            let val: Val = i.into_val(&env);
            let cat_res: Result<InvoiceCategory, _> = InvoiceCategory::try_from_val(&env, &val);
            assert!(cat_res.is_err(), "Arbitrary category {} should fail", i);
        }
    }
}
