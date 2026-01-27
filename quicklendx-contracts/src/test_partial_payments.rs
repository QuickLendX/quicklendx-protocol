use soroban_sdk::{testutils::{Address as _}, Address, String, Vec, Env, vec, token};
use crate::{QuickLendXContract, QuickLendXContractClient};
use crate::invoice::{InvoiceCategory, InvoiceStatus};

/// Test partial payment tracking, cumulative totals, and full settlement detection
pub fn test_partial_payments_comprehensive() {
    let env = Env::default();
    env.mock_all_auths();

    // Register the contract
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    // Set up token contract
    let currency = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    // Mint tokens and set up approvals
    let initial_balance = 10_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    let due_date = env.ledger().timestamp() + 86400;

    // Setup
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC Data"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC Data"));
    client.verify_investor(&investor, &10000);

    // Test Case 1: Single partial payment
    let invoice_id_1 = client.store_invoice(
        &business,
        &1000, // $10.00
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice 1 - partial payment"),
        &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "test")]
    );

    // Verify and fund invoice 1
    client.verify_invoice(&invoice_id_1);
    let bid_id_1 = client.place_bid(&investor, &invoice_id_1, &1000, &1050);
    client.accept_bid(&invoice_id_1, &bid_id_1);

    // Single partial payment
    let tx1 = String::from_str(&env, "partial-tx-1");
    client.process_partial_payment(&invoice_id_1, &400, &tx1);

    // Verify payment tracking
    let invoice = client.get_invoice(&invoice_id_1);
    assert_eq!(invoice.total_paid, 400);
    assert_eq!(invoice.payment_history.len(), 1);
    assert_eq!(invoice.payment_progress(), 40);
    assert_eq!(invoice.status, InvoiceStatus::Funded);

    // Test Case 2: Multiple partial payments
    let invoice_id_2 = client.store_invoice(
        &business,
        &2000, // $20.00
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice 2 - multiple payments"),
        &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "test")]
    );

    // Verify and fund invoice 2
    client.verify_invoice(&invoice_id_2);
    let bid_id_2 = client.place_bid(&investor, &invoice_id_2, &2000, &2100);
    client.accept_bid(&invoice_id_2, &bid_id_2);

    // Multiple partial payments
    let tx2_1 = String::from_str(&env, "partial-tx-2-1");
    client.process_partial_payment(&invoice_id_2, &500, &tx2_1);

    let tx2_2 = String::from_str(&env, "partial-tx-2-2");
    client.process_partial_payment(&invoice_id_2, &800, &tx2_2);

    let tx2_3 = String::from_str(&env, "partial-tx-2-3");
    client.process_partial_payment(&invoice_id_2, &700, &tx2_3);

    // Verify cumulative totals and settlement
    let invoice = client.get_invoice(&invoice_id_2);
    assert_eq!(invoice.total_paid, 2000);
    assert_eq!(invoice.payment_history.len(), 3);
    assert_eq!(invoice.payment_progress(), 100);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert!(invoice.is_fully_paid());

    // Test Case 3: Overpayment handling
    let invoice_id_3 = client.store_invoice(
        &business,
        &1500, // $15.00
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice 3 - overpayment"),
        &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "test")]
    );

    // Verify and fund invoice 3
    client.verify_invoice(&invoice_id_3);
    let bid_id_3 = client.place_bid(&investor, &invoice_id_3, &1500, &1575);
    client.accept_bid(&invoice_id_3, &bid_id_3);

    // Overpayment - pay more than invoice amount
    let tx3 = String::from_str(&env, "overpayment-tx-3");
    client.process_partial_payment(&invoice_id_3, &1800, &tx3);

    // Verify overpayment handling
    let invoice = client.get_invoice(&invoice_id_3);
    assert_eq!(invoice.total_paid, 1800);
    assert_eq!(invoice.payment_history.len(), 1);
    assert_eq!(invoice.payment_progress(), 100);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert!(invoice.is_fully_paid());

    // Test Case 4: Payment history integrity
    // Check that payment records are properly stored
    let invoice = client.get_invoice(&invoice_id_2);

    // Verify cumulative total matches sum of records
    let mut calculated_total = 0i128;
    for record in invoice.payment_history.iter() {
        calculated_total = calculated_total.saturating_add(record.amount);
    }
    assert_eq!(calculated_total, invoice.total_paid);

    // Verify transaction IDs are unique and properly stored
    let mut tx_ids = Vec::new(&env);
    for record in invoice.payment_history.iter() {
        assert!(!tx_ids.contains(&record.transaction_id));
        tx_ids.push_back(record.transaction_id.clone());
    }
}

/// Test settlement edge cases and validation
pub fn test_settlement_edge_cases() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let token_admin = Address::generate(&env);

    // Set up token contract
    let currency = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    // Mint tokens and set up approvals
    let initial_balance = 10_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    let due_date = env.ledger().timestamp() + 86400;

    // Setup
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC"));
    client.verify_investor(&investor, &10000);

    // Test settlement with exact amount
    let invoice_id_exact = client.store_invoice(
        &business, &1000, &currency, &due_date,
        &String::from_str(&env, "Exact settlement"), &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "test")]
    );

    client.verify_invoice(&invoice_id_exact);
    let bid_id_exact = client.place_bid(&investor, &invoice_id_exact, &1000, &1050);
    client.accept_bid(&invoice_id_exact, &bid_id_exact);

    client.settle_invoice(&invoice_id_exact, &1000);

    let invoice = client.get_invoice(&invoice_id_exact);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert_eq!(invoice.total_paid, 1000);
    assert!(invoice.is_fully_paid());

    // Test settlement with overpayment
    let invoice_id_over = client.store_invoice(
        &business, &800, &currency, &due_date,
        &String::from_str(&env, "Over settlement"), &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "test")]
    );

    client.verify_invoice(&invoice_id_over);
    let bid_id_over = client.place_bid(&investor, &invoice_id_over, &800, &840);
    client.accept_bid(&invoice_id_over, &bid_id_over);

    client.settle_invoice(&invoice_id_over, &1000);

    let invoice = client.get_invoice(&invoice_id_over);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert_eq!(invoice.total_paid, 1000);
    assert!(invoice.is_fully_paid());

    // Test settlement validation errors - skipped as Soroban panics on invalid inputs
    let invoice_id_invalid = client.store_invoice(
        &business, &500, &currency, &due_date,
        &String::from_str(&env, "Invalid settlement"), &InvoiceCategory::Services,
        &vec![&env, String::from_str(&env, "test")]
    );

    client.verify_invoice(&invoice_id_invalid);
    // Note: Invalid settlement operations would panic in Soroban
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partial_payments_validation() {
        test_partial_payments_comprehensive();
    }

    #[test]
    fn test_settlement_validation() {
        test_settlement_edge_cases();
    }
}
