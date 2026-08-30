#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::payments::{create_escrow, refund_escrow, release_escrow};
use crate::storage::InvoiceStorage;
use crate::types::{Invoice, InvoiceStatus};
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

fn setup() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    (env, contract_id)
}

fn create_dummy_invoice(
    env: &Env,
    invoice_id: &BytesN<32>,
    business: &Address,
    currency: &Address,
    investor: Option<Address>,
) {
    let invoice = Invoice {
        invoice_id: invoice_id.clone(),
        business: business.clone(),
        amount: 1000,
        due_date: 0,
        status: InvoiceStatus::Verified,
        currency: currency.clone(),
        metadata_hash: String::from_str(env, "hash"),
        created_at: 0,
        funded_amount: 0,
        funded_at: None,
        investor: investor,
        category: String::from_str(env, "cat"),
        tags: Vec::new(env),
        insurance_opt_in: false,
        payment_history: Vec::new(env),
        origination_fee_bps: None,
        early_payment_discount_bps: None,
        insurance_premium: 0,
        escrow_id: None,
        repayment_type: crate::types::RepaymentType::Full,
    };
    InvoiceStorage::store_invoice(env, &invoice);
}

#[test]
fn test_create_escrow_cross_tenant_rejected() {
    let (env, contract_id) = setup();
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let actual_business = Address::generate(&env);
    let forged_business = Address::generate(&env);
    let currency = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        create_dummy_invoice(&env, &invoice_id, &actual_business, &currency, None);

        // Attempt to create escrow with a business address that doesn't own the invoice
        let result = create_escrow(
            &env,
            &invoice_id,
            &investor,
            &forged_business,
            100,
            &currency,
        );
        assert_eq!(result, Err(QuickLendXError::Unauthorized));

        // Attempt to create escrow with wrong currency
        let wrong_currency = Address::generate(&env);
        let result2 = create_escrow(
            &env,
            &invoice_id,
            &investor,
            &actual_business,
            100,
            &wrong_currency,
        );
        assert_eq!(result2, Err(QuickLendXError::InvalidCurrency));
    });
}

#[test]
fn test_release_escrow_cross_tenant_rejected() {
    let (env, contract_id) = setup();
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Create an invoice with one business
        create_dummy_invoice(&env, &invoice_id, &business, &currency, None);

        // Create the escrow directly (simulating a state where escrow.business differs from invoice.business,
        // which shouldn't happen with our new create_escrow checks, but we test the release boundary).
        let forged_business = Address::generate(&env);
        let escrow_id = BytesN::from_array(&env, &[2u8; 32]);
        let escrow = crate::payments::Escrow {
            escrow_id,
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            business: forged_business.clone(),
            amount: 100,
            currency: currency.clone(),
            created_at: 0,
            status: crate::payments::EscrowStatus::Held,
        };
        crate::payments::EscrowStorage::store_escrow(&env, &escrow);

        // Releasing should fail because escrow.business != invoice.business
        let result = release_escrow(&env, &invoice_id);
        assert_eq!(result, Err(QuickLendXError::Unauthorized));
    });
}

#[test]
fn test_refund_escrow_cross_tenant_rejected() {
    let (env, contract_id) = setup();
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let actual_investor = Address::generate(&env);
    let forged_investor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // Invoice is funded by actual_investor
        create_dummy_invoice(
            &env,
            &invoice_id,
            &business,
            &currency,
            Some(actual_investor.clone()),
        );

        let escrow_id = BytesN::from_array(&env, &[2u8; 32]);
        let escrow = crate::payments::Escrow {
            escrow_id,
            invoice_id: invoice_id.clone(),
            investor: forged_investor.clone(),
            business: business.clone(),
            amount: 100,
            currency: currency.clone(),
            created_at: 0,
            status: crate::payments::EscrowStatus::Held,
        };
        crate::payments::EscrowStorage::store_escrow(&env, &escrow);

        // Refunding should fail because escrow.investor != invoice.investor
        let result = refund_escrow(&env, &invoice_id);
        assert_eq!(result, Err(QuickLendXError::Unauthorized));
    });
}

#[test]
fn test_release_and_refund_escrow_missing_state_handled() {
    let (env, contract_id) = setup();
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    env.as_contract(&contract_id, || {
        // Missing escrow gracefully errors out rather than panic/unwrap
        let result_release = release_escrow(&env, &invoice_id);
        assert_eq!(result_release, Err(QuickLendXError::StorageKeyNotFound));

        let result_refund = refund_escrow(&env, &invoice_id);
        assert_eq!(result_refund, Err(QuickLendXError::StorageKeyNotFound));
    });
}
