//! Protocol limits boundary tests — Issue #826
//!
//! Covers: invoice amount bounds, due-date horizon, string/vector limits.


use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::protocol_limits::{
    check_string_length, MAX_DESCRIPTION_LENGTH, MAX_DISPUTE_EVIDENCE_LENGTH,
    MAX_DISPUTE_REASON_LENGTH, MAX_FEEDBACK_LENGTH, MAX_KYC_DATA_LENGTH, MAX_NAME_LENGTH,
    MAX_NOTES_LENGTH, MAX_REJECTION_REASON_LENGTH, MAX_TAG_LENGTH, MAX_ADDRESS_LENGTH,
    MAX_TAX_ID_LENGTH,
};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, QuickLendXContractClient<'static\>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn make_str(env: &Env, n: usize) -> String {
    String::from_str(env, &"a".repeat(n))
}

fn verified_business(env: &Env, client: &QuickLendXContractClient, admin: &Address) -> Address {
    let b = Address::generate(env);
    client.submit_kyc_application(&b, &String::from_str(env, "kyc"));
    client.verify_business(admin, &b);
    b
}

fn store(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    desc: &str,
    tags: Vec<String>,
) -> Result<soroban_sdk::BytesN<32>, crate::errors::QuickLendXError> {
    let currency = Address::generate(env);
    let due = env.ledger().timestamp() + 86_400;
    client.try_store_invoice(
        business,
        &amount,
        &currency,
        &due,
        &String::from_str(env, desc),
        &InvoiceCategory::Services,
        &tags,
    )
}

// ===========================================================================
// 1. INVOICE AMOUNT BOUNDS
// ===========================================================================

#[test]
fn test_amount_zero_rejected() {
    let (env, client, _) = setup();
    let b = Address::generate(&env);
        store(&env, &client, &b, 0, "desc", Vec::new(&env)),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_amount_negative_rejected() {
    let (env, client, _) = setup();
    let b = Address::generate(&env);
        store(&env, &client, &b, -1, "desc", Vec::new(&env)),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_amount_below_min_rejected() {
    let (env, client, admin) = setup();
    // Set min to 100
    client.set_protocol_limits(&admin, &100i128, &365u64, &0u64);
    let b = Address::generate(&env);
        store(&env, &client, &b, 99, "desc", Vec::new(&env)),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_amount_at_min_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &100i128, &365u64, &0u64);
    let b = Address::generate(&env);
    assert!(store(&env, &client, &b, 100, "desc", Vec::new(&env)).is_ok());
}

#[test]
fn test_amount_above_min_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    assert!(store(&env, &client, &b, 1_000_000, "desc", Vec::new(&env)).is_ok());
}

#[test]
fn test_amount_i128_max_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &1i128, &365u64, &0u64);
    let b = Address::generate(&env);
    assert!(store(&env, &client, &b, i128::MAX, "desc", Vec::new(&env)).is_ok());
}

// ===========================================================================
// 2. DUE-DATE HORIZON BOUNDS
// ===========================================================================

#[test]
fn test_due_date_in_past_rejected() {
    let (env, client, _) = setup();
    let b = Address::generate(&env);
    let currency = Address::generate(&env);
    let past = env.ledger().timestamp().saturating_sub(1);
    let result = client.try_store_invoice(
        &b, &100i128, &currency, &past,
        &String::from_str(&env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_due_date_at_now_rejected() {
    let (env, client, _) = setup();
    let b = Address::generate(&env);
    let currency = Address::generate(&env);
    let now = env.ledger().timestamp();
    let result = client.try_store_invoice(
        &b, &100i128, &currency, &now,
        &String::from_str(&env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_due_date_beyond_max_horizon_rejected() {
    let (env, client, admin) = setup();
    // 1-day horizon
    client.set_protocol_limits(&admin, &10i128, &1u64, &0u64);
    let b = Address::generate(&env);
    let currency = Address::generate(&env);
    let too_far = env.ledger().timestamp() + 2 * 86_400;
    let result = client.try_store_invoice(
        &b, &10i128, &currency, &too_far,
        &String::from_str(&env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_due_date_within_max_horizon_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let currency = Address::generate(&env);
    let ok = env.ledger().timestamp() + 86_400;
        &b, &10i128, &currency, &ok,
        &String::from_str(&env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ).is_ok());
}

#[test]
fn test_due_date_exactly_at_max_horizon_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &730u64, &0u64);
    let b = Address::generate(&env);
    let currency = Address::generate(&env);
    let exactly = env.ledger().timestamp() + 730 * 86_400;
        &b, &10i128, &currency, &exactly,
        &String::from_str(&env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ).is_ok());
}

// ===========================================================================
// 3. PROTOCOL LIMITS PARAMETER BOUNDS
// ===========================================================================

#[test]
fn test_set_limits_min_amount_zero_rejected() {
    let (_, client, admin) = setup();
        client.try_set_protocol_limits(&admin, &0i128, &365u64, &0u64),
        Err(Ok(QuickLendXError::InvalidAmount))
    );
}

#[test]
fn test_set_limits_min_amount_negative_rejected() {
    let (_, client, admin) = setup();
        client.try_set_protocol_limits(&admin, &-1i128, &365u64, &0u64),
        Err(Ok(QuickLendXError::InvalidAmount))
    );
}

#[test]
fn test_set_limits_max_due_days_zero_rejected() {
    let (_, client, admin) = setup();
        client.try_set_protocol_limits(&admin, &10i128, &0u64, &0u64),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid))
    );
}

#[test]
fn test_set_limits_max_due_days_731_rejected() {
    let (_, client, admin) = setup();
        client.try_set_protocol_limits(&admin, &10i128, &731u64, &0u64),
        Err(Ok(QuickLendXError::InvoiceDueDateInvalid))
    );
}

#[test]
fn test_set_limits_max_due_days_730_accepted() {
    let (_, client, admin) = setup();
    assert!(client.try_set_protocol_limits(&admin, &10i128, &730u64, &0u64).is_ok());
}

#[test]
fn test_set_limits_grace_period_above_max_rejected() {
    let (_, client, admin) = setup();
        client.try_set_protocol_limits(&admin, &10i128, &365u64, &2_592_001u64),
        Err(Ok(QuickLendXError::InvalidTimestamp))
    );
}

#[test]
fn test_set_limits_grace_period_at_max_accepted() {
    let (_, client, admin) = setup();
    assert!(client.try_set_protocol_limits(&admin, &10i128, &365u64, &2_592_000u64).is_ok());
}

#[test]
fn test_set_limits_grace_exceeds_horizon_rejected() {
    let (_, client, admin) = setup();
    // 1-day horizon, 2-day grace — impossible combination
        client.try_set_protocol_limits(&admin, &10i128, &1u64, &172_801u64),
        Err(Ok(QuickLendXError::InvalidTimestamp))
    );
}

#[test]
fn test_set_limits_non_admin_rejected() {
    let (env, client, _) = setup();
    let stranger = Address::generate(&env);
        client.try_set_protocol_limits(&stranger, &10i128, &365u64, &0u64),
        Err(Ok(QuickLendXError::NotAdmin))
    );
}

// ===========================================================================
// 4. DESCRIPTION STRING LIMITS
// ===========================================================================

#[test]
fn test_description_empty_rejected() {
    let (env, client, _) = setup();
    let b = Address::generate(&env);
    let currency = Address::generate(&env);
    let due = env.ledger().timestamp() + 86_400;
    let result = client.try_store_invoice(
        &b, &10i128, &currency, &due,
        &String::from_str(&env, ""),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
}

#[test]
fn test_description_at_max_length_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let currency = Address::generate(&env);
    let due = env.ledger().timestamp() + 86_400;
    let desc = make_str(&env, MAX_DESCRIPTION_LENGTH as usize);
        &b, &10i128, &currency, &due,
        &desc,
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ).is_ok());
}

// ===========================================================================
// 5. TAG VECTOR AND STRING LIMITS
// ===========================================================================

#[test]
fn test_tag_count_at_limit_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let mut tags = Vec::new(&env);
    for i in 0..10u32 {
        tags.push_back(String::from_str(&env, &format!("tag{}", i)));
    }
    assert!(store(&env, &client, &b, 10, "desc", tags).is_ok());
}

#[test]
fn test_tag_count_above_limit_rejected() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let mut tags = Vec::new(&env);
    for i in 0..11u32 {
        tags.push_back(String::from_str(&env, &format!("tag{}", i)));
    }
        store(&env, &client, &b, 10, "desc", tags),
        Err(QuickLendXError::TagLimitExceeded)
    );
}

#[test]
fn test_tag_length_at_max_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(make_str(&env, MAX_TAG_LENGTH as usize));
    assert!(store(&env, &client, &b, 10, "desc", tags).is_ok());
}

#[test]
fn test_tag_length_above_max_rejected() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(make_str(&env, MAX_TAG_LENGTH as usize + 1));
    assert!(store(&env, &client, &b, 10, "desc", tags).is_err());
}

#[test]
fn test_tag_empty_rejected() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, ""));
    assert!(store(&env, &client, &b, 10, "desc", tags).is_err());
}

#[test]
fn test_duplicate_tags_rejected() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "dup"));
    tags.push_back(String::from_str(&env, "dup"));
    assert!(store(&env, &client, &b, 10, "desc", tags).is_err());
}

#[test]
fn test_zero_tags_accepted() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    assert!(store(&env, &client, &b, 10, "desc", Vec::new(&env)).is_ok());
}

// ===========================================================================
// 6. KYC DATA STRING LIMITS
// ===========================================================================

#[test]
fn test_kyc_data_at_max_accepted() {
    let (env, client, _) = setup();
    let b = Address::generate(&env);
    let data = make_str(&env, MAX_KYC_DATA_LENGTH as usize);
    assert!(client.try_submit_kyc_application(&b, &data).is_ok());
}

#[test]
fn test_kyc_data_above_max_rejected() {
    let (env, client, _) = setup();
    let b = Address::generate(&env);
    let data = make_str(&env, MAX_KYC_DATA_LENGTH as usize + 1);
    assert!(client.try_submit_kyc_application(&b, &data).is_err());
}

#[test]
fn test_kyc_data_empty_accepted() {
    // Empty KYC is allowed at the string-length level (business logic may differ)
    let (env, client, _) = setup();
    let b = Address::generate(&env);
    assert!(client.try_submit_kyc_application(&b, &String::from_str(&env, "")).is_ok());
}

// ===========================================================================
// 7. REJECTION REASON STRING LIMITS
// ===========================================================================

#[test]
fn test_rejection_reason_at_max_accepted() {
    let (env, client, admin) = setup();
    let b = Address::generate(&env);
    client.submit_kyc_application(&b, &String::from_str(&env, "kyc"));
    let reason = make_str(&env, MAX_REJECTION_REASON_LENGTH as usize);
    assert!(client.try_reject_business(&admin, &b, &reason).is_ok());
}

#[test]
fn test_rejection_reason_above_max_rejected() {
    let (env, client, admin) = setup();
    let b = Address::generate(&env);
    client.submit_kyc_application(&b, &String::from_str(&env, "kyc"));
    let reason = make_str(&env, MAX_REJECTION_REASON_LENGTH as usize + 1);
    assert!(client.try_reject_business(&admin, &b, &reason).is_err());
}

// ===========================================================================
// 8. DISPUTE STRING LIMITS
// ===========================================================================

fn funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (soroban_sdk::BytesN<32>, Address) {
    let b = verified_business(env, client, admin);
    let currency = Address::generate(env);
    client.add_currency(admin, &currency);
    let due = env.ledger().timestamp() + 86_400;
    let id = client.upload_invoice(
        &b, &10i128, &currency, &due,
        &String::from_str(env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    (id, b)
}

#[test]
fn test_dispute_reason_at_max_accepted() {
    let (env, client, admin) = setup();
    let (id, b) = funded_invoice(&env, &client, &admin);
    let reason = make_str(&env, MAX_DISPUTE_REASON_LENGTH as usize);
    let evidence = String::from_str(&env, "evidence");
    assert!(client.try_create_dispute(&id, &b, &reason, &evidence).is_ok());
}

#[test]
fn test_dispute_reason_above_max_rejected() {
    let (env, client, admin) = setup();
    let (id, b) = funded_invoice(&env, &client, &admin);
    let reason = make_str(&env, MAX_DISPUTE_REASON_LENGTH as usize + 1);
    let evidence = String::from_str(&env, "evidence");
    assert!(client.try_create_dispute(&id, &b, &reason, &evidence).is_err());
}

#[test]
fn test_dispute_evidence_at_max_accepted() {
    let (env, client, admin) = setup();
    let (id, b) = funded_invoice(&env, &client, &admin);
    let reason = String::from_str(&env, "reason");
    let evidence = make_str(&env, MAX_DISPUTE_EVIDENCE_LENGTH as usize);
    assert!(client.try_create_dispute(&id, &b, &reason, &evidence).is_ok());
}

#[test]
fn test_dispute_evidence_above_max_rejected() {
    let (env, client, admin) = setup();
    let (id, b) = funded_invoice(&env, &client, &admin);
    let reason = String::from_str(&env, "reason");
    let evidence = make_str(&env, MAX_DISPUTE_EVIDENCE_LENGTH as usize + 1);
    assert!(client.try_create_dispute(&id, &b, &reason, &evidence).is_err());
}

#[test]
fn test_dispute_reason_empty_rejected() {
    let (env, client, admin) = setup();
    let (id, b) = funded_invoice(&env, &client, &admin);
    let result = client.try_create_dispute(
        &id, &b,
        &String::from_str(&env, ""),
        &String::from_str(&env, "evidence"),
    );
    assert!(result.is_err());
}

#[test]
fn test_dispute_evidence_empty_rejected() {
    let (env, client, admin) = setup();
    let (id, b) = funded_invoice(&env, &client, &admin);
    let result = client.try_create_dispute(
        &id, &b,
        &String::from_str(&env, "reason"),
        &String::from_str(&env, ""),
    );
    assert!(result.is_err());
}

// ===========================================================================
// 9. check_string_length UNIT TESTS
// ===========================================================================

#[test]
fn test_check_string_length_at_limit_ok() {
    let env = Env::default();
    let s = make_str(&env, 50);
    assert!(check_string_length(&s, 50).is_ok());
}

#[test]
fn test_check_string_length_above_limit_err() {
    let env = Env::default();
    let s = make_str(&env, 51);
        check_string_length(&s, 50),
        Err(QuickLendXError::InvalidDescription)
    );
}

#[test]
fn test_check_string_length_zero_limit_empty_ok() {
    let env = Env::default();
    let s = String::from_str(&env, "");
    assert!(check_string_length(&s, 0).is_ok());
}

// ===========================================================================
// 10. LIMITS APPLIED CONSISTENTLY ACROSS store_invoice AND upload_invoice
// ===========================================================================

#[test]
fn test_upload_invoice_enforces_min_amount() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &500i128, &365u64, &0u64);
    let b = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let due = env.ledger().timestamp() + 86_400;
    let result = client.try_upload_invoice(
        &b, &499i128, &currency, &due,
        &String::from_str(&env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_upload_invoice_enforces_due_date_horizon() {
    let (env, client, admin) = setup();
    client.set_protocol_limits(&admin, &10i128, &1u64, &0u64);
    let b = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    let too_far = env.ledger().timestamp() + 2 * 86_400;
    let result = client.try_upload_invoice(
        &b, &10i128, &currency, &too_far,
        &String::from_str(&env, "desc"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvoiceDueDateInvalid)));
}

#[test]
fn test_limits_update_takes_effect_immediately() {
    let (env, client, admin) = setup();
    // Initially allow amount=10
    client.set_protocol_limits(&admin, &10i128, &365u64, &0u64);
    let b = Address::generate(&env);
    assert!(store(&env, &client, &b, 10, "desc", Vec::new(&env)).is_ok());

    // Raise min to 1000 — now 10 is rejected
    client.set_protocol_limits(&admin, &1000i128, &365u64, &0u64);
        store(&env, &client, &b, 10, "desc", Vec::new(&env)),
        Err(QuickLendXError::InvalidAmount)
    );
}
