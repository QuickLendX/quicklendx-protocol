use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn create_string(env: &Env, len: u32) -> String {
    let s = "a".repeat(len as usize);
    String::from_str(env, &s)
}

fn verified_business(env: &Env, client: &QuickLendXContractClient<'_>, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

fn verified_investor(env: &Env, client: &QuickLendXContractClient<'_>) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &1_000_000);
    investor
}

fn verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient<'_>,
    admin: &Address,
) -> BytesN<32> {
    let business = verified_business(env, client, admin);
    let currency = Address::generate(env);
    let invoice_id = client
        .try_store_invoice(
            &business,
            &10_000,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(env, "Verified invoice"),
            &InvoiceCategory::Services,
            &Vec::new(env),
        )
        .expect("host error")
        .expect("conversion error");
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn assert_contract_err<T: core::fmt::Debug, C: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<Result<T, C>, Result<QuickLendXError, E>>,
    expected: QuickLendXError,
) {
    match result {
        Err(Ok(err)) => assert_eq!(err, expected),
        other => panic!("expected contract error {:?}, got {:?}", expected, other),
    }
}

fn assert_no_host_error<T: core::fmt::Debug, C: core::fmt::Debug, E: core::fmt::Debug>(
    result: Result<Result<T, C>, Result<QuickLendXError, E>>,
) {
    match result {
        Ok(Ok(_)) | Err(Ok(_)) => {}
        Ok(Err(err)) => panic!("unexpected conversion error: {:?}", err),
        Err(Err(err)) => panic!("unexpected host error: {:?}", err),
    }
}

#[test]
fn test_store_invoice_zero_amount() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &0,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvalidAmount,
    );
}

#[test]
fn test_store_invoice_negative_amount() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &-1,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvalidAmount,
    );
}

#[test]
fn test_store_invoice_i128_max_amount() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    assert_no_host_error(client.try_store_invoice(
        &business,
        &i128::MAX,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ));
}

#[test]
fn test_upload_invoice_zero_amount() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_upload_invoice(
            &business,
            &0,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvalidAmount,
    );
}

#[test]
fn test_upload_invoice_negative_amount() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_upload_invoice(
            &business,
            &-1,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvalidAmount,
    );
}

#[test]
fn test_place_bid_zero_amount() {
    let (env, client, admin) = setup();
    let investor = verified_investor(&env, &client);
    let invoice_id = verified_invoice(&env, &client, &admin);

    assert_contract_err(
        client.try_place_bid(&investor, &invoice_id, &0, &1),
        QuickLendXError::InvalidAmount,
    );
}

#[test]
fn test_place_bid_negative_amount() {
    let (env, client, admin) = setup();
    let investor = verified_investor(&env, &client);
    let invoice_id = verified_invoice(&env, &client, &admin);

    assert_contract_err(
        client.try_place_bid(&investor, &invoice_id, &-1, &1),
        QuickLendXError::InvalidAmount,
    );
}

#[test]
fn test_set_bid_ttl_zero() {
    let (_, client, _) = setup();
    assert_contract_err(
        client.try_set_bid_ttl_days(&0),
        QuickLendXError::InvalidBidTtl,
    );
}

#[test]
fn test_set_bid_ttl_over_30() {
    let (_, client, _) = setup();
    assert_contract_err(
        client.try_set_bid_ttl_days(&31),
        QuickLendXError::InvalidBidTtl,
    );
}

#[test]
fn test_set_platform_fee_over_max() {
    let (_, client, _) = setup();
    assert_contract_err(
        client.try_set_platform_fee(&1_001),
        QuickLendXError::InvalidFeeBasisPoints,
    );
}

#[test]
fn test_store_invoice_past_due_date() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    env.ledger().set_timestamp(10_000);

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &100,
            &currency,
            &9_999,
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvoiceDueDateInvalid,
    );
}

#[test]
fn test_store_invoice_zero_due_date() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    env.ledger().set_timestamp(10_000);

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &100,
            &currency,
            &0,
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvoiceDueDateInvalid,
    );
}

#[test]
fn test_upload_invoice_past_due_date() {
    let (env, client, admin) = setup();
    env.ledger().set_timestamp(10_000);
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_upload_invoice(
            &business,
            &100,
            &currency,
            &9_999,
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvoiceDueDateInvalid,
    );
}

#[test]
fn test_upload_invoice_zero_due_date() {
    let (env, client, admin) = setup();
    env.ledger().set_timestamp(10_000);
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_upload_invoice(
            &business,
            &100,
            &currency,
            &0,
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvoiceDueDateInvalid,
    );
}

#[test]
fn test_store_invoice_empty_description() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &100,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, ""),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvalidDescription,
    );
}

#[test]
fn test_upload_invoice_empty_description() {
    let (env, client, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_upload_invoice(
            &business,
            &100,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, ""),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvalidDescription,
    );
}

#[test]
fn test_store_invoice_oversized_description() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &100,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &create_string(&env, 1_025),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        QuickLendXError::InvalidDescription,
    );
}

#[test]
fn test_kyc_oversized_data() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);

    assert_contract_err(
        client.try_submit_kyc_application(&business, &create_string(&env, 5_001)),
        QuickLendXError::InvalidDescription,
    );
}

#[test]
fn test_rejection_oversized_reason() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));

    assert_contract_err(
        client.try_reject_business(&admin, &business, &create_string(&env, 501)),
        QuickLendXError::InvalidDescription,
    );
}

#[test]
fn test_store_invoice_too_many_tags() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let mut tags = Vec::new(&env);
    for _ in 0..11 {
        tags.push_back(String::from_str(&env, "tag"));
    }

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &100,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &tags,
        ),
        QuickLendXError::TagLimitExceeded,
    );
}

#[test]
fn test_store_invoice_oversized_tag() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(create_string(&env, 51));

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &100,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &tags,
        ),
        QuickLendXError::InvalidTag,
    );
}

#[test]
fn test_store_invoice_empty_tag_after_normalization() {
    let (env, client, _) = setup();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let mut tags = Vec::new(&env);
    tags.push_back(String::from_str(&env, "   "));

    assert_contract_err(
        client.try_store_invoice(
            &business,
            &100,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &tags,
        ),
        QuickLendXError::InvalidTag,
    );
}

#[test]
fn test_error_code_consistency() {
    assert_eq!(QuickLendXError::InvoiceNotFound as u32, 1000);
    assert_eq!(QuickLendXError::InvoiceAmountInvalid as u32, 1003);
    assert_eq!(QuickLendXError::Unauthorized as u32, 1100);
    assert_eq!(QuickLendXError::NotAdmin as u32, 1103);
    assert_eq!(QuickLendXError::InvalidAmount as u32, 1200);
    assert_eq!(QuickLendXError::InvalidTimestamp as u32, 1203);
    assert_eq!(QuickLendXError::InvalidDescription as u32, 1204);
    assert_eq!(QuickLendXError::InvalidTag as u32, 1800);
    assert_eq!(QuickLendXError::InvalidBidTtl as u32, 1408);
    assert_eq!(QuickLendXError::InvalidFeeBasisPoints as u32, 1852);
}

#[test]
fn test_no_panics_on_invalid_inputs() {
    let (env, client, admin) = setup();
    let invalid_id = BytesN::from_array(&env, &[0u8; 32]);
    let business = Address::generate(&env);
    let verified = verified_business(&env, &client, &admin);
    let currency = Address::generate(&env);
    env.ledger().set_timestamp(10_000);

    assert_no_host_error(client.try_get_invoice(&invalid_id));
    assert_no_host_error(client.try_store_invoice(
        &business,
        &0,
        &currency,
        &(env.ledger().timestamp() + 1),
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ));
    assert_no_host_error(client.try_store_invoice(
        &business,
        &-1,
        &currency,
        &(env.ledger().timestamp() + 1),
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ));
    assert_no_host_error(client.try_store_invoice(
        &business,
        &1000,
        &currency,
        &(env.ledger().timestamp() + 1),
        &String::from_str(&env, ""),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ));
    assert_no_host_error(client.try_upload_invoice(
        &verified,
        &100,
        &currency,
        &0,
        &String::from_str(&env, "Test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ));
    assert_no_host_error(client.try_set_bid_ttl_days(&0));
    assert_no_host_error(client.try_set_bid_ttl_days(&31));
}
