#![cfg(test)]

use crate::{
    invoice::{InvoiceCategory, InvoiceStatus},
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
};
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
    (env, client, admin)
}

fn create_invoice_at(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    currency: &Address,
    timestamp: u64,
) -> BytesN<32> {
    env.ledger().set_timestamp(timestamp);
    client.store_invoice(
        business,
        &1_000i128,
        currency,
        &(timestamp + 86_400),
        &String::from_str(env, "invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &None,
        &None,
    )
}

#[test]
fn test_business_invoices_cursored_pagination() {
    let (env, client, admin) = setup();
    let currency = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);

    let id1 = create_invoice_at(&env, &client, &business, &currency, 1_000);
    let id2 = create_invoice_at(&env, &client, &business, &currency, 2_000);

    let page1 = client.get_business_invoices_cursored(
        &business,
        &Option::<InvoiceStatus>::None,
        &0u32,
        &1u32,
        &None,
    );

    assert_eq!(page1.items.len(), 1);
    assert_eq!(page1.total_count, 2);
    assert!(page1.has_more);
    let gen = page1.generation;

    // Stable cursor works
    let page2 = client.get_business_invoices_cursored(
        &business,
        &Option::<InvoiceStatus>::None,
        &1u32,
        &1u32,
        &Some(gen),
    );
    assert_eq!(page2.items.len(), 1);
    assert_eq!(page2.total_count, 2);
    assert!(!page2.has_more);

    // Destabilized cursor
    create_invoice_at(&env, &client, &business, &currency, 3_000);

    let err = client
        .try_get_business_invoices_cursored(
            &business,
            &Option::<InvoiceStatus>::None,
            &1u32,
            &1u32,
            &Some(gen),
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::UnstableCursor);
}

#[test]
fn test_available_invoices_cursored_pagination() {
    let (env, client, admin) = setup();
    let currency = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);

    let id1 = create_invoice_at(&env, &client, &business, &currency, 1_000);

    let page1 = client.get_available_invoices_cursored(&None, &None, &None, &0u32, &1u32, &None);
    let gen = page1.generation;

    let page2 =
        client.get_available_invoices_cursored(&None, &None, &None, &0u32, &1u32, &Some(gen));
    assert_eq!(page2.generation, gen);
}
