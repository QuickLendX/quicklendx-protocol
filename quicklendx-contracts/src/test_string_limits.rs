use super::*;
use crate::invoice::{InvoiceMetadata, LineItemRecord};
use crate::protocol_limits::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin)
}

fn verify_business(
    client: &QuickLendXContractClient<'static>,
    admin: &Address,
    business: &Address,
) {
    client.submit_kyc_application(business, &String::from_str(admin.env(), "KYC"));
    client.verify_business(admin, business);
}

fn create_string(env: &Env, len: u32) -> String {
    let s = "a".repeat(len as usize);
    String::from_str(env, &s)
}

#[test]
fn test_invoice_metadata_limits() {
    let env = Env::default();

    // Exactly at limits
    let metadata = InvoiceMetadata {
        customer_name: create_string(&env, MAX_NAME_LENGTH),
        customer_address: create_string(&env, MAX_ADDRESS_LENGTH),
        tax_id: create_string(&env, MAX_TAX_ID_LENGTH),
        line_items: Vec::new(&env),
        notes: create_string(&env, MAX_NOTES_LENGTH),
    };
    assert!(metadata.validate().is_ok());

    // Over name limit
    let mut bad_metadata = metadata.clone();
    bad_metadata.customer_name = create_string(&env, MAX_NAME_LENGTH + 1);
    assert!(bad_metadata.validate().is_err());

    // Over address limit
    let mut bad_metadata = metadata.clone();
    bad_metadata.customer_address = create_string(&env, MAX_ADDRESS_LENGTH + 1);
    assert!(bad_metadata.validate().is_err());

    // Over tax_id limit
    let mut bad_metadata = metadata.clone();
    bad_metadata.tax_id = create_string(&env, MAX_TAX_ID_LENGTH + 1);
    assert!(bad_metadata.validate().is_err());

    // Over notes limit
    let mut bad_metadata = metadata.clone();
    bad_metadata.notes = create_string(&env, MAX_NOTES_LENGTH + 1);
    assert!(bad_metadata.validate().is_err());
}

#[test]
fn test_line_item_description_limit() {
    let env = Env::default();
    let item = LineItemRecord(create_string(&env, MAX_DESCRIPTION_LENGTH), 1, 100, 100);
    let mut line_items = Vec::new(&env);
    line_items.push_back(item);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "Test"),
        customer_address: String::from_str(&env, "Test"),
        tax_id: String::from_str(&env, "Test"),
        line_items,
        notes: String::from_str(&env, "Test"),
    };
    assert!(metadata.validate().is_ok());

    // Over description limit
    let mut bad_line_items = Vec::new(&env);
    bad_line_items.push_back(LineItemRecord(
        create_string(&env, MAX_DESCRIPTION_LENGTH + 1),
        1,
        100,
        100,
    ));
    let mut bad_metadata = metadata.clone();
    bad_metadata.line_items = bad_line_items;
    assert!(bad_metadata.validate().is_err());
}

#[test]
fn test_kyc_data_limit() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);

    // Exactly at limit
    let kyc_data = create_string(&env, MAX_KYC_DATA_LENGTH);
    assert!(client
        .try_submit_kyc_application(&business, &kyc_data)
        .is_ok());

    // Over limit
    let business_2 = Address::generate(&env);
    let long_kyc = create_string(&env, MAX_KYC_DATA_LENGTH + 1);
    assert!(client
        .try_submit_kyc_application(&business_2, &long_kyc)
        .is_err());
}

#[test]
fn test_rejection_reason_limit() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));

    // Exactly at limit
    let reason = create_string(&env, MAX_REJECTION_REASON_LENGTH);
    assert!(client
        .try_reject_business(&admin, &business, &reason)
        .is_ok());

    // Over limit
    let business_2 = Address::generate(&env);
    client.submit_kyc_application(&business_2, &String::from_str(&env, "KYC"));
    let long_reason = create_string(&env, MAX_REJECTION_REASON_LENGTH + 1);
    assert!(client
        .try_reject_business(&admin, &business_2, &long_reason)
        .is_err());
}

#[test]
fn test_tag_limits() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    verify_business(&client, &admin, &business);

    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let category = crate::invoice::InvoiceCategory::Services;
    let desc = String::from_str(&env, "Invoice with tags");
    let currency = Address::generate(&env);

    // Exactly at tag length limit
    let mut tags = Vec::new(&env);
    tags.push_back(create_string(&env, MAX_TAG_LENGTH));
    assert!(client
        .try_upload_invoice(&business, &amount, &currency, &due_date, &desc, &category, &tags)
        .is_ok());

    // Over tag length limit
    let mut bad_tags = Vec::new(&env);
    bad_tags.push_back(create_string(&env, MAX_TAG_LENGTH + 1));
    assert!(client
        .try_upload_invoice(&business, &amount, &currency, &due_date, &desc, &category, &bad_tags)
        .is_err());
}

#[test]
fn test_dispute_limits() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    verify_business(&client, &admin, &business);

    let amount = 1000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let category = crate::invoice::InvoiceCategory::Services;
    let desc = String::from_str(&env, "Invoice for dispute");
    let currency = Address::generate(&env);
    let tags = Vec::new(&env);

    let invoice_id = client.upload_invoice(
        &business, &amount, &currency, &due_date, &desc, &category, &tags,
    );

    // Create dispute exactly at limit (using business as creator)
    let reason = create_string(&env, MAX_DISPUTE_REASON_LENGTH);
    let evidence = create_string(&env, MAX_DISPUTE_EVIDENCE_LENGTH);
    assert!(client
        .try_create_dispute(&invoice_id, &business, &reason, &evidence)
        .is_ok());
}

// ============================================================================
// TAG NORMALIZATION + STRING LIMIT INTERACTION TESTS (#527)
// ============================================================================

/// A 50-char uppercase tag normalizes to a 50-char lowercase tag — still valid.
#[test]
fn test_tag_at_limit_uppercase_normalizes_valid() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // 50 uppercase 'A' characters — normalizes to 50 lowercase 'a' characters.
    let mut s = std::string::String::with_capacity(50);
    for _ in 0..50 {
        s.push('A');
    }
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, &s));

    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &crate::invoice::InvoiceCategory::Services,
        &tags,
    );
    assert!(
        res.is_ok(),
        "50-char uppercase tag should normalize to valid 50-char lowercase"
    );
}

/// A tag with leading/trailing spaces that trims to exactly 50 chars is valid.
#[test]
fn test_tag_trim_to_limit_valid() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    // Build " " + 50 'a' chars + " " = 52 bytes, normalizes to 50 bytes.
    let mut s = std::string::String::with_capacity(52);
    s.push(' ');
    for _ in 0..50 {
        s.push('a');
    }
    s.push(' ');
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, &s));

    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &crate::invoice::InvoiceCategory::Services,
        &tags,
    );
    assert!(
        res.is_ok(),
        "tag that trims to exactly 50 chars should be valid"
    );
}

/// A tag with spaces only is rejected after normalization.
#[test]
fn test_tag_spaces_only_invalid_after_norm() {
    let (env, client, _admin) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "     "));

    let res = client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &due_date,
        &String::from_str(&env, "Desc"),
        &crate::invoice::InvoiceCategory::Services,
        &tags,
    );
    assert!(res.is_err());
    assert_eq!(
        res.err().unwrap().unwrap(),
        crate::errors::QuickLendXError::InvalidTag
    );
}
