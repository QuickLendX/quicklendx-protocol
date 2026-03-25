#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, String, Vec};

use crate::QuickLendXContract;

fn setup(env: &Env) -> (crate::QuickLendXContractClient<'static>, Address, Address, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = crate::QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = Address::generate(env);
    
    // 1. Initialize admin
    client.initialize_admin(&admin);
    
    // 2. Verify business
    client.submit_kyc_application(&business, &String::from_str(env, "{}"));
    client.verify_business(&admin, &business);
    
    // 3. Verify investor
    client.submit_investor_kyc(&investor, &String::from_str(env, "{}"));
    client.verify_investor(&investor, &1_000_000i128);
    
    (client, admin, business, investor, currency)
}

#[test]
fn test_refund_only_by_admin_or_owner() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, business, investor, currency) = setup(&env);

    let due_date = env.ledger().timestamp() + 86400;
    let amount = 1_000i128;

    // 1. Create and Fund Invoice
    let invoice_id: BytesN<32> = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Hardening test"),
        &InvoiceCategory::Technology,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    let bid_id: BytesN<32> = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    // 2. Try unauthorized refund (Investor)
    let stranger = Address::generate(&env);
    let result = client.try_refund_escrow_funds(&invoice_id, &stranger);
    assert!(
        matches!(result, Err(Ok(QuickLendXError::Unauthorized))),
        "Strangers should not be allowed to refund"
    );

    let result = client.try_refund_escrow_funds(&invoice_id, &investor);
    assert!(
        matches!(result, Err(Ok(QuickLendXError::Unauthorized))),
        "Investors should not be allowed to refund"
    );

    // 3. Authorized refund (Business Owner)
    client.refund_escrow_funds(&invoice_id, &business);
    
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
}

#[test]
fn test_refund_fails_on_already_paid_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, business, investor, currency) = setup(&env);

    let due_date = env.ledger().timestamp() + 86400;
    let amount = 1_000i128;

    let invoice_id: BytesN<32> = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Hardening test"),
        &InvoiceCategory::Technology,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    let bid_id: BytesN<32> = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    // Bypass buggy settle_invoice (which has double require_auth in current codebase)
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Paid);

    // Invoice status is now Paid; refund must fail
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);

    let result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(
        matches!(result, Err(Ok(QuickLendXError::InvalidStatus))),
        "Refund must fail if invoice is already Paid"
    );
}

#[test]
fn test_refund_status_bucket_integrity() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, business, investor, currency) = setup(&env);

    let due_date = env.ledger().timestamp() + 86400;
    let amount = 1_000i128;

    let invoice_id: BytesN<32> = client.upload_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Status bucket test"),
        &InvoiceCategory::Technology,
        &Vec::new(&env),
    );
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Verified);
    let bid_id: BytesN<32> = client.place_bid(&investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    // Initial check
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Funded), 1);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Refunded), 0);

    // Execute refund
    client.refund_escrow_funds(&invoice_id, &admin);

    // Final check
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Funded), 0);
    assert_eq!(client.get_invoice_count_by_status(&InvoiceStatus::Refunded), 1);
    
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
}
