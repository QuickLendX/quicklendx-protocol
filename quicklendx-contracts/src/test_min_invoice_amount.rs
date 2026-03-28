#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

#[test]
fn test_store_invoice_rejects_below_minimum_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.set_protocol_limits(&admin, &1_000i128, &365u64, &0u64);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let result = client.try_store_invoice(
        &business,
        &999i128,
        &currency,
        &due_date,
        &String::from_str(&env, "below min"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
}

#[test]
fn test_store_invoice_allows_at_minimum_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.set_protocol_limits(&admin, &1_000i128, &365u64, &0u64);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    assert!(client
        .try_store_invoice(
            &business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "at min"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());
}

#[test]
fn test_upload_invoice_enforces_minimum_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.set_protocol_limits(&admin, &1_000i128, &365u64, &0u64);

    // Verify a business for upload_invoice flow
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let below = client.try_upload_invoice(
        &business,
        &999i128,
        &currency,
        &due_date,
        &String::from_str(&env, "below min"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(below, Err(Ok(QuickLendXError::InvalidAmount)));

    assert!(client
        .try_upload_invoice(
            &business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "at min"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        )
        .is_ok());
}
