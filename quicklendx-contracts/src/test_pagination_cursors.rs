#![cfg(test)]

use crate::{
    invoice::{InvoiceCategory, InvoiceStatus},
    QuickLendXContract, QuickLendXContractClient, QuickLendXError,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    // Needs some init if required
    (env, client)
}

fn create_invoice_at(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    timestamp: u64,
) -> BytesN<32> {
    env.ledger().set_timestamp(timestamp);
    client.store_invoice(
        business,
        &1_000i128,
        &Address::generate(env),
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
    let (env, client) = setup();
    let business = Address::generate(&env);

    create_invoice_at(&env, &client, &business, 1_000);
    create_invoice_at(&env, &client, &business, 2_000);

    let page1 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &None::<String>,
        &1u32,
    );

    assert_eq!(page1.items.len(), 1);
    assert_eq!(page1.total_count, 2);
    assert!(page1.has_more);
    let cursor = page1.next_cursor.clone().unwrap();

    let page2 = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &Some(cursor.clone()),
        &1u32,
    );
    assert_eq!(page2.items.len(), 1);
    assert_eq!(page2.total_count, 2);
    assert!(!page2.has_more);

    create_invoice_at(&env, &client, &business, 3_000);
    let err = client
        .try_get_business_invoices_paged(
            &business,
            &Option::<InvoiceStatus>::None,
            &Some(cursor),
            &1u32,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::UnstableCursor);
}

#[test]
fn test_business_invoice_pagination_rejects_malformed_cursor() {
    let (env, client) = setup();
    let business = Address::generate(&env);
    let error = client
        .try_get_business_invoices_paged(
            &business,
            &Option::<InvoiceStatus>::None,
            &Some(String::from_str(&env, "not-a-cursor")),
            &1u32,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(error, QuickLendXError::InvalidCursor);
}

#[test]
fn test_business_invoice_cursor_is_bound_to_business_and_filter() {
    let (env, client) = setup();
    let business = Address::generate(&env);
    let other_business = Address::generate(&env);
    create_invoice_at(&env, &client, &business, 1_000);
    create_invoice_at(&env, &client, &business, 2_000);

    let page = client.get_business_invoices_paged(
        &business,
        &Option::<InvoiceStatus>::None,
        &None::<String>,
        &1u32,
    );
    let cursor = page.next_cursor.unwrap();

    let other_business_error = client
        .try_get_business_invoices_paged(
            &other_business,
            &Option::<InvoiceStatus>::None,
            &Some(cursor.clone()),
            &1u32,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(other_business_error, QuickLendXError::UnstableCursor);

    let other_filter_error = client
        .try_get_business_invoices_paged(
            &business,
            &Some(InvoiceStatus::Pending),
            &Some(cursor),
            &1u32,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(other_filter_error, QuickLendXError::UnstableCursor);
}
