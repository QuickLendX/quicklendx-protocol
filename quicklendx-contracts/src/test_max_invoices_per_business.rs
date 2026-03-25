#![cfg(test)]
extern crate std;
use std::format;

use crate::{
    invoice::{InvoiceCategory, InvoiceStatus, InvoiceStorage},
    protocol_limits::ProtocolLimitsContract,
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    client.set_admin(&admin);
    client.initialize_protocol_limits(&admin, &1i128, &365u64, &86400u64);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    (env, client, admin, business, currency)
}

fn upload_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    suffix: &str,
) -> soroban_sdk::BytesN<32> {
    client.upload_invoice(
        business,
        &1_000i128,
        currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(env, suffix),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

#[test]
fn test_create_invoices_up_to_limit_succeeds() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &1i128, &365u64, &86400u64, &3u32);

    for i in 0..3 {
        let invoice_id =
            upload_invoice(&env, &client, &business, &currency, &format!("Invoice {i}"));
        assert!(InvoiceStorage::get_invoice(&env, &invoice_id).is_some());
    }

    assert_eq!(
        InvoiceStorage::get_business_invoices(&env, &business).len(),
        3
    );
    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business),
        3
    );
}

#[test]
fn test_next_invoice_after_limit_fails_with_clear_error() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &1i128, &365u64, &86400u64, &2u32);

    upload_invoice(&env, &client, &business, &currency, "Invoice 1");
    upload_invoice(&env, &client, &business, &currency, "Invoice 2");

    let result = client.try_upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Invoice 3"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().unwrap(),
        QuickLendXError::MaxInvoicesPerBusinessExceeded
    );
}

#[test]
fn test_cancelled_invoice_frees_up_slot() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &1i128, &365u64, &86400u64, &1u32);

    let invoice_id = upload_invoice(&env, &client, &business, &currency, "Invoice 1");

    let first_retry = client.try_upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(first_retry.is_err());

    client.cancel_invoice(&invoice_id);

    let invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
    assert_eq!(invoice.status, InvoiceStatus::Cancelled);

    let replacement_id = upload_invoice(&env, &client, &business, &currency, "Replacement");
    assert_ne!(replacement_id, invoice_id);
    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business),
        1
    );
}

#[test]
fn test_limit_update_changes_capacity() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &1i128, &365u64, &86400u64, &1u32);
    upload_invoice(&env, &client, &business, &currency, "Invoice 1");

    let blocked = client.try_upload_invoice(
        &business,
        &1_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Invoice 2"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(blocked.is_err());

    client.update_limits_max_invoices(&admin, &1i128, &365u64, &86400u64, &3u32);

    let limits = ProtocolLimitsContract::get_protocol_limits(env.clone());
    assert_eq!(limits.max_invoices_per_business, 3);

    upload_invoice(&env, &client, &business, &currency, "Invoice 2");
    upload_invoice(&env, &client, &business, &currency, "Invoice 3");
    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business),
        3
    );
}
