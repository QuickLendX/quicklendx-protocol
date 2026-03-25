#![cfg(test)]

extern crate std;
use std::format;

use crate::{
    invoice::{Invoice, InvoiceCategory, InvoiceStatus, InvoiceStorage},
    protocol_limits::ProtocolLimitsContract,
    verification::{BusinessVerificationStatus, BusinessVerificationStorage},
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Verified business required for `upload_invoice`.
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);

    (env, client, admin, business, currency)
}

fn upload_basic_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    amount: i128,
) -> soroban_sdk::BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    client.upload_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    )
}

#[test]
fn test_upload_invoice_enforces_max_invoices_per_business() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &10i128, &365u64, &0u64, &3u32);

    for _ in 0..3 {
        upload_basic_invoice(&env, &client, &business, &currency, 10);
    }

    let due_date = env.ledger().timestamp() + 86_400;
    let result = client.try_upload_invoice(
        &business,
        &10i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(
        result,
        Err(Ok(QuickLendXError::MaxInvoicesPerBusinessExceeded))
    );
}

#[test]
fn test_cancelled_invoices_free_up_slots() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &10i128, &365u64, &0u64, &2u32);

    let invoice1 = upload_basic_invoice(&env, &client, &business, &currency, 10);
    let _invoice2 = upload_basic_invoice(&env, &client, &business, &currency, 10);

    // Limit reached
    let due_date = env.ledger().timestamp() + 86_400;
    assert_eq!(
        client.try_upload_invoice(
            &business,
            &10i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        ),
        Err(Ok(QuickLendXError::MaxInvoicesPerBusinessExceeded))
    );

    // Cancel one invoice -> slot freed
    client.cancel_invoice(&invoice1);
    let cancelled = InvoiceStorage::get_invoice(&env, &invoice1).unwrap();
    assert_eq!(cancelled.status, InvoiceStatus::Cancelled);

    upload_basic_invoice(&env, &client, &business, &currency, 10);
    assert_eq!(InvoiceStorage::count_active_business_invoices(&env, &business), 2);
}

#[test]
fn test_paid_invoices_free_up_slots() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &10i128, &365u64, &0u64, &2u32);

    let invoice1 = upload_basic_invoice(&env, &client, &business, &currency, 10);
    let _invoice2 = upload_basic_invoice(&env, &client, &business, &currency, 10);

    // Mark invoice1 as paid (simulate settlement)
    let mut inv = InvoiceStorage::get_invoice(&env, &invoice1).unwrap();
    inv.mark_as_paid(&env, business.clone(), env.ledger().timestamp());
    InvoiceStorage::update_invoice(&env, &inv);

    // Should allow creating a new invoice now
    upload_basic_invoice(&env, &client, &business, &currency, 10);
    assert_eq!(InvoiceStorage::count_active_business_invoices(&env, &business), 2);
}

#[test]
fn test_limit_zero_disables_max_invoices_check() {
    let (env, client, admin, business, currency) = setup();

    client.update_limits_max_invoices(&admin, &10i128, &365u64, &0u64, &0u32);

    for _ in 0..10 {
        upload_basic_invoice(&env, &client, &business, &currency, 10);
    }

    assert_eq!(InvoiceStorage::count_active_business_invoices(&env, &business), 10);
}
