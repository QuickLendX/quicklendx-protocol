#![cfg(test)]

use super::*;
use soroban_sdk::{Address, BytesN, Env, String};

#[test]
fn test_basic_invoice_creation() {
    let env = Env::default();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let amount = 1000;
    let due_date = env.ledger().timestamp() + 86400; // 1 day from now
    let description = String::from_str(&env, "Test invoice for services");
    let category = invoice::InvoiceCategory::Services;
    let tags = vec![&env, String::from_str(&env, "test")];

    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &description,
        &category,
        &tags,
    );

    // Verify invoice was stored
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, amount);
    assert_eq!(invoice.currency, currency);
    assert_eq!(invoice.due_date, due_date);
    assert_eq!(invoice.description, description);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());
}
