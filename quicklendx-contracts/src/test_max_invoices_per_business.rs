#![cfg(test)]

use crate::{
    invoice::{InvoiceCategory, InvoiceStatus, InvoiceStorage},
    protocol_limits::ProtocolLimitsContract,
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

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

    client.initialize_admin(&admin);
    client.add_currency(&admin, &currency);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    (env, client, admin, business, currency)
}

fn invoice_args(env: &Env) -> (i128, u64, String, InvoiceCategory, Vec<String>) {
    (
        1_000,
        env.ledger().timestamp() + 86_400,
        String::from_str(env, "Test invoice"),
        InvoiceCategory::Services,
        Vec::new(env),
    )
}

#[test]
fn test_create_invoices_up_to_limit_succeeds() {
    let (env, client, admin, business, currency) = setup();
    client.update_limits_max_invoices(&admin, &10, &365, &86_400, &5);

    let (amount, due_date, description, category, tags) = invoice_args(&env);
    for _ in 0..5 {
        client.upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
    }

    assert_eq!(
        InvoiceStorage::get_business_invoices(&env, &business).len(),
        5
    );
    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business),
        5
    );
}

#[test]
fn test_next_invoice_after_limit_fails_with_clear_error() {
    let (env, client, admin, business, currency) = setup();
    client.update_limits_max_invoices(&admin, &10, &365, &86_400, &3);

    let (amount, due_date, description, category, tags) = invoice_args(&env);
    for _ in 0..3 {
        client.upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
    }

    let result = client.try_upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    let err = result.err().expect("expected invoice limit error");
    assert_eq!(
        err.expect("expected contract error"),
        QuickLendXError::MaxInvoicesPerBusinessExceeded
    );
}

#[test]
fn test_cancelled_invoices_free_slot() {
    let (env, client, admin, business, currency) = setup();
    client.update_limits_max_invoices(&admin, &10, &365, &86_400, &2);

    let (amount, due_date, description, category, tags) = invoice_args(&env);
    let first = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    assert!(client
        .try_upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .is_err());

    client.cancel_invoice(&first);
    assert_eq!(
        InvoiceStorage::get_invoice(&env, &first).unwrap().status,
        InvoiceStatus::Cancelled
    );

    client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );
    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business),
        2
    );
}

#[test]
fn test_limit_zero_means_unlimited() {
    let (env, client, admin, business, currency) = setup();
    client.update_limits_max_invoices(&admin, &10, &365, &86_400, &0);

    let (amount, due_date, description, category, tags) = invoice_args(&env);
    for _ in 0..10 {
        client.upload_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
    }

    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business),
        10
    );
    assert_eq!(
        ProtocolLimitsContract::get_protocol_limits(env.clone()).max_invoices_per_business,
        0
    );
}

#[test]
fn test_multiple_businesses_have_independent_limits() {
    let (env, client, admin, business_one, currency) = setup();
    let business_two = Address::generate(&env);
    client.submit_kyc_application(&business_two, &String::from_str(&env, "Business 2 KYC"));
    client.verify_business(&admin, &business_two);
    client.update_limits_max_invoices(&admin, &10, &365, &86_400, &2);

    let (amount, due_date, description, category, tags) = invoice_args(&env);
    for _ in 0..2 {
        client.upload_invoice(
            &business_one,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
    }

    assert!(client
        .try_upload_invoice(
            &business_one,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
        .is_err());

    for _ in 0..2 {
        client.upload_invoice(
            &business_two,
            &amount,
            &currency,
            &due_date,
            &description,
            &category,
            &tags,
        );
    }

    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business_one),
        2
    );
    assert_eq!(
        InvoiceStorage::count_active_business_invoices(&env, &business_two),
        2
    );
}
