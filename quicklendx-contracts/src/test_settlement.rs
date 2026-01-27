use super::*;
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use crate::profits::calculate_profit;
use crate::settlement::settle_invoice;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

/// Helper function to verify investor for testing
fn verify_investor_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    limit: i128,
) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(investor, &limit);
}

/// Helper function to set up a funded invoice for testing
fn setup_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    investor: &Address,
    currency: &Address,
    invoice_amount: i128,
    investment_amount: i128,
) -> BytesN<32> {
    let admin = Address::generate(env);
    client.set_admin(&admin);

    // Set up token with balances
    let token_client = token::Client::new(env, currency);
    let sac_client = token::StellarAssetClient::new(env, currency);

    let initial_balance = 10_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);

    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(business, &env.current_contract_address(), &initial_balance, &expiration);
    token_client.approve(investor, &env.current_contract_address(), &initial_balance, &expiration);

    // Verify business
    client.submit_kyc_application(business, &String::from_str(env, "KYC data"));
    client.verify_business(&admin, business);

    // Create and verify invoice
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        business,
        &invoice_amount,
        currency,
        &due_date,
        &String::from_str(env, "Test invoice for settlement"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );

    client.verify_invoice(&invoice_id);

    // Verify investor and place bid
    verify_investor_for_test(env, client, investor, 10_000);
    let bid_id = client.place_bid(investor, &invoice_id, &investment_amount, &invoice_amount);
    client.accept_bid(&invoice_id, &bid_id);

    invoice_id
}

/// Test that unfunded invoices cannot be settled
#[test]
fn test_cannot_settle_unfunded_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Verify business
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    // Create invoice but don't fund it
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Unfunded invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.verify_invoice(&invoice_id);

    // Verify invoice is in Verified status, not Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    assert_eq!(invoice.funded_amount, 0);
    assert!(invoice.investor.is_none());

    // Attempt to settle should fail
    let result = client.try_settle_invoice(&invoice_id, &1_000);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::InvalidStatus,
        "Should fail with InvalidStatus when trying to settle unfunded invoice"
    );
}

/// Test that settlement with Pending status fails
#[test]
fn test_cannot_settle_pending_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    // Verify business
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    // Create invoice but don't verify it (stays in Pending)
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Pending invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // Verify invoice is in Pending status
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);

    // Attempt to settle should fail
    let result = client.try_settle_invoice(&invoice_id, &1_000);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::InvalidStatus,
        "Should fail with InvalidStatus when trying to settle pending invoice"
    );
}

/// Test that payout matches expected return calculation
#[test]
fn test_payout_matches_expected_return() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set up funded invoice: $1000 invoice, $900 investment
    let invoice_amount = 1_000i128;
    let investment_amount = 900i128;
    let payment_amount = 1_000i128; // Full payment

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        invoice_amount,
        investment_amount,
    );

    // Get initial balances
    let token_client = token::Client::new(&env, &currency);
    let initial_business_balance = token_client.balance(&business);
    let initial_investor_balance = token_client.balance(&investor);
    let platform_address = env.current_contract_address();
    let initial_platform_balance = token_client.balance(&platform_address);

    // Calculate expected returns using the same logic as settlement
    let (expected_investor_return, expected_platform_fee) =
        calculate_profit(&env, investment_amount, payment_amount);

    // Ensure business has enough balance to pay
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&business, &payment_amount);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &payment_amount,
        &expiration,
    );

    // Settle the invoice
    client.settle_invoice(&invoice_id, &payment_amount);

    // Verify final balances
    let final_business_balance = token_client.balance(&business);
    let final_investor_balance = token_client.balance(&investor);
    let final_platform_balance = token_client.balance(&platform_address);

    // Business should have paid the full amount
    assert_eq!(
        initial_business_balance + payment_amount - payment_amount,
        final_business_balance,
        "Business balance should decrease by payment amount"
    );

    // Investor should receive the calculated return
    assert_eq!(
        final_investor_balance - initial_investor_balance,
        expected_investor_return,
        "Investor should receive the calculated return amount"
    );

    // Platform should receive the calculated fee
    assert_eq!(
        final_platform_balance - initial_platform_balance,
        expected_platform_fee,
        "Platform should receive the calculated fee"
    );

    // Verify the sum: investor return + platform fee should equal payment amount
    assert_eq!(
        expected_investor_return + expected_platform_fee,
        payment_amount,
        "Investor return + platform fee should equal payment amount"
    );
}

/// Test payout calculation with profit
#[test]
fn test_payout_with_profit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set up: $1000 invoice, $800 investment, $1000 payment (profit = $200)
    let invoice_amount = 1_000i128;
    let investment_amount = 800i128;
    let payment_amount = 1_000i128;

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        invoice_amount,
        investment_amount,
    );

    let token_client = token::Client::new(&env, &currency);
    let initial_investor_balance = token_client.balance(&investor);

    // Calculate expected returns
    let (expected_investor_return, expected_platform_fee) =
        calculate_profit(&env, investment_amount, payment_amount);

    // Profit = payment - investment = 1000 - 800 = 200
    // Platform fee (2%) = 200 * 0.02 = 4
    // Investor return = 1000 - 4 = 996

    // Ensure business has enough balance
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&business, &payment_amount);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &payment_amount,
        &expiration,
    );

    // Settle
    client.settle_invoice(&invoice_id, &payment_amount);

    // Verify investor received correct amount
    let final_investor_balance = token_client.balance(&investor);
    let investor_received = final_investor_balance - initial_investor_balance;

    assert_eq!(
        investor_received, expected_investor_return,
        "Investor should receive the calculated return with profit"
    );
    assert!(
        investor_received > investment_amount,
        "Investor should receive more than their investment when there's profit"
    );
}

/// Test status transitions during settlement
#[test]
fn test_status_transitions_correct() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Verify invoice is in Funded status
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Invoice should be in Funded status after funding"
    );

    // Verify investment is Active
    let investment = env
        .as_contract(&contract_id, || {
            InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
        })
        .expect("Investment should exist");
    assert_eq!(
        investment.status,
        InvestmentStatus::Active,
        "Investment should be Active before settlement"
    );

    // Set up payment
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let payment_amount = 1_000i128;
    sac_client.mint(&business, &payment_amount);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &payment_amount,
        &expiration,
    );

    // Settle the invoice
    client.settle_invoice(&invoice_id, &payment_amount);

    // Verify invoice status changed to Paid
    let settled_invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        settled_invoice.status,
        InvoiceStatus::Paid,
        "Invoice status should transition to Paid after settlement"
    );
    assert!(
        settled_invoice.settled_at.is_some(),
        "Invoice should have settled_at timestamp after settlement"
    );

    // Verify investment status changed to Completed
    let settled_investment = env
        .as_contract(&contract_id, || {
            InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
        })
        .expect("Investment should exist");
    assert_eq!(
        settled_investment.status,
        InvestmentStatus::Completed,
        "Investment status should transition to Completed after settlement"
    );

    // Verify invoice is removed from Funded list and added to Paid list
    let funded_invoices = client.get_invoices_by_status(&InvoiceStatus::Funded);
    assert!(
        !funded_invoices.contains(&invoice_id),
        "Invoice should be removed from Funded list"
    );

    let paid_invoices = client.get_invoices_by_status(&InvoiceStatus::Paid);
    assert!(
        paid_invoices.contains(&invoice_id),
        "Invoice should be added to Paid list"
    );
}

/// Test that double-settlement is prevented
#[test]
fn test_prevents_double_settle() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Set up payment
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let payment_amount = 1_000i128;
    sac_client.mint(&business, &payment_amount);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &payment_amount,
        &expiration,
    );

    // First settlement should succeed
    client.settle_invoice(&invoice_id, &payment_amount);

    // Verify invoice is now Paid
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);

    // Attempt second settlement should fail
    let result = client.try_settle_invoice(&invoice_id, &payment_amount);
    assert!(
        result.is_err(),
        "Second settlement attempt should fail"
    );
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::InvalidStatus,
        "Should fail with InvalidStatus when trying to settle already-paid invoice"
    );

    // Verify invoice status is still Paid (not changed)
    let invoice_after_attempt = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice_after_attempt.status,
        InvoiceStatus::Paid,
        "Invoice status should remain Paid after failed settlement attempt"
    );
}

/// Test settlement with payment amount less than investment amount fails
#[test]
fn test_settlement_payment_too_low() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set up: $1000 invoice, $900 investment
    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Attempt to settle with amount less than investment (should fail)
    let low_payment = 800i128; // Less than investment amount of 900

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&business, &low_payment);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &low_payment,
        &expiration,
    );

    let result = client.try_settle_invoice(&invoice_id, &low_payment);
    assert!(
        result.is_err(),
        "Settlement with payment less than investment should fail"
    );
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::PaymentTooLow,
        "Should fail with PaymentTooLow error"
    );

    // Verify invoice is still Funded (not settled)
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice.status,
        InvoiceStatus::Funded,
        "Invoice should remain Funded after failed settlement"
    );
}

/// Test settlement with payment amount less than invoice amount fails
#[test]
fn test_settlement_payment_less_than_invoice() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    // Set up: $1000 invoice, $900 investment
    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Attempt to settle with amount less than invoice amount (should fail)
    let low_payment = 950i128; // More than investment but less than invoice

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&business, &low_payment);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &low_payment,
        &expiration,
    );

    let result = client.try_settle_invoice(&invoice_id, &low_payment);
    assert!(
        result.is_err(),
        "Settlement with payment less than invoice amount should fail"
    );
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::PaymentTooLow,
        "Should fail with PaymentTooLow error"
    );
}

/// Test settlement with zero payment amount fails
#[test]
fn test_settlement_zero_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Attempt to settle with zero amount (should fail)
    let result = client.try_settle_invoice(&invoice_id, &0);
    assert!(
        result.is_err(),
        "Settlement with zero payment should fail"
    );
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::InvalidAmount,
        "Should fail with InvalidAmount error"
    );
}

/// Test settlement with negative payment amount fails
#[test]
fn test_settlement_negative_payment() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Attempt to settle with negative amount (should fail)
    let result = client.try_settle_invoice(&invoice_id, &-100);
    assert!(
        result.is_err(),
        "Settlement with negative payment should fail"
    );
    assert_eq!(
        result.unwrap_err(),
        QuickLendXError::InvalidAmount,
        "Should fail with InvalidAmount error"
    );
}

/// Test settlement updates total_paid correctly
#[test]
fn test_settlement_updates_total_paid() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Verify initial total_paid is 0
    let invoice_before = client.get_invoice(&invoice_id);
    assert_eq!(invoice_before.total_paid, 0);

    // Set up payment
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let payment_amount = 1_000i128;
    sac_client.mint(&business, &payment_amount);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &payment_amount,
        &expiration,
    );

    // Settle
    client.settle_invoice(&invoice_id, &payment_amount);

    // Verify total_paid is updated
    let invoice_after = client.get_invoice(&invoice_id);
    assert_eq!(
        invoice_after.total_paid,
        payment_amount,
        "total_paid should equal payment amount after settlement"
    );
}

/// Test settlement with partial payments already recorded
#[test]
fn test_settlement_with_existing_partial_payments() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000,
        900,
    );

    // Make a partial payment first
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let partial_amount = 400i128;
    sac_client.mint(&business, &partial_amount);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &partial_amount,
        &expiration,
    );

    client.process_partial_payment(&invoice_id, &partial_amount, &String::from_str(&env, "tx-1"));

    // Verify partial payment was recorded
    let invoice_after_partial = client.get_invoice(&invoice_id);
    assert_eq!(invoice_after_partial.total_paid, partial_amount);
    assert_eq!(invoice_after_partial.status, InvoiceStatus::Funded);

    // Now settle with full payment amount
    let remaining_amount = 600i128;
    sac_client.mint(&business, &remaining_amount);
    token_client.approve(
        &business,
        &env.current_contract_address(),
        &remaining_amount,
        &expiration,
    );

    let full_payment = 1_000i128;
    client.settle_invoice(&invoice_id, &full_payment);

    // Verify final state
    let final_invoice = client.get_invoice(&invoice_id);
    assert_eq!(final_invoice.total_paid, full_payment);
    assert_eq!(final_invoice.status, InvoiceStatus::Paid);
}
