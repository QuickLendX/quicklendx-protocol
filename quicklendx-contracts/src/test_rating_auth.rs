#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

fn make_funded_invoice(
    env: &Env,
    contract_id: &Address,
    investor: &Address,
    business: &Address,
) -> (Invoice, BytesN<32>) {
    let (invoice, invoice_id) = env.as_contract(contract_id, || {
        let invoice = Invoice::new(
            env,
            business.clone(),
            10_000i128,
            Address::generate(env),
            env.ledger().timestamp() + 86_400,
            String::from_str(env, "Rated invoice"),
            InvoiceCategory::Services,
            soroban_sdk::Vec::new(env),
            None,
            None,
            None,
        )
        .unwrap();
        let invoice_id = invoice.id.clone();
        (invoice, invoice_id)
    });
    let mut invoice = invoice;
    invoice.status = InvoiceStatus::Funded;
    invoice.investor = Some(investor.clone());
    (invoice, invoice_id)
}

#[test]
fn test_third_party_cannot_rate_invoice() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let attacker = Address::generate(&env);

    let (mut invoice, invoice_id) = env.as_contract(&contract_id, || {
        let invoice = make_funded_invoice(&env, &contract_id, &investor, &business).0;
        let invoice_id = invoice.id.clone();
        crate::storage::InvoiceStorage::store_invoice(&env, &invoice);
        (invoice, invoice_id)
    });

    let result = invoice.add_rating(1, String::from_str(&env, "fake"), attacker.clone(), 1);
    assert_eq!(result, Err(QuickLendXError::NotRater));
    assert_eq!(invoice.ratings.len(), 0);

    let stored = env.as_contract(&contract_id, || crate::storage::InvoiceStorage::get_invoice(&env, &invoice_id).unwrap());
    assert_eq!(stored.ratings.len(), 0);
}

#[test]
fn test_investor_can_rate_once() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let (mut invoice, _invoice_id) = make_funded_invoice(&env, &contract_id, &investor, &business);

    assert!(invoice
        .add_rating(4, String::from_str(&env, "good"), investor.clone(), 2)
        .is_ok());
    assert_eq!(invoice.average_rating, Some(4));
    assert_eq!(invoice.total_ratings, 1);
    assert_eq!(
        invoice.add_rating(5, String::from_str(&env, "again"), investor, 3),
        Err(QuickLendXError::AlreadyRated)
    );
}

#[test]
fn test_business_cannot_rate_own_invoice() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let (mut invoice, _invoice_id) = make_funded_invoice(&env, &contract_id, &investor, &business);

    assert_eq!(
        invoice.add_rating(5, String::from_str(&env, "business"), business, 7),
        Err(QuickLendXError::NotRater)
    );
    assert_eq!(invoice.ratings.len(), 0);
}

#[test]
fn test_rating_requires_signature() {
    let env = Env::default();
    let contract_id = env.register(crate::QuickLendXContract, ());
    let client = crate::QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = env.as_contract(&contract_id, || {
        let mut invoice = Invoice::new(
            &env,
            business.clone(),
            10_000i128,
            Address::generate(&env),
            env.ledger().timestamp() + 86_400,
            String::from_str(&env, "Signed rating"),
            InvoiceCategory::Services,
            soroban_sdk::Vec::new(&env),
            None,
            None,
            None,
        )
        .unwrap();
        invoice.status = InvoiceStatus::Funded;
        invoice.investor = Some(investor.clone());
        crate::storage::InvoiceStorage::store_invoice(&env, &invoice);
        invoice.id.clone()
    });

    let result = client.try_add_invoice_rating(
        &invoice_id,
        &5u32,
        &String::from_str(&env, "signed"),
        &investor,
    );
    assert!(result.is_err(), "missing signer auth must fail");
}

#[test]
fn test_unfunded_invoice_cannot_be_rated() {
    let env = Env::default();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let contract_id = env.register(crate::QuickLendXContract, ());
    let mut invoice = env.as_contract(&contract_id, || {
        Invoice::new(
            &env,
            business,
            10_000i128,
            Address::generate(&env),
            env.ledger().timestamp() + 86_400,
            String::from_str(&env, "Pending invoice"),
            InvoiceCategory::Services,
            soroban_sdk::Vec::new(&env),
            None,
            None,
            None,
        )
        .unwrap()
    });
    invoice.status = InvoiceStatus::Verified;
    invoice.investor = Some(investor.clone());

    assert_eq!(
        invoice.add_rating(5, String::from_str(&env, "nope"), investor, 9),
        Err(QuickLendXError::NotFunded)
    );
}
