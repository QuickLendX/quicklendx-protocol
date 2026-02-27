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

/// Settlement deadline helper should align with protocol limits (due_date + grace).
#[test]
fn test_get_invoice_settlement_deadline_matches_protocol_limits() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);

    // Configure protocol limits with a small, known grace period.
    let min_amount: i128 = 1;
    let max_due_days: u64 = 365;
    let grace_period: u64 = 3600; // 1 hour
    let _ = client.try_initialize_protocol_limits(&admin, &min_amount, &max_due_days, &grace_period);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let now = env.ledger().timestamp();
    let due_date = now + 10_000;
    let invoice_id = client.store_invoice(
        &business,
        &1_000,
        &currency,
        &due_date,
        &String::from_str(&env, "Deadline invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let deadline = client
        .get_invoice_settlement_deadline(&invoice_id)
        .expect("deadline query should succeed");
    assert_eq!(
        deadline,
        due_date.saturating_add(grace_period),
        "Settlement/default deadline must equal due_date + grace_period"
    );
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

// ============================================================================
// calculate_profit integration with settlement (#341)
// ============================================================================
// NOTE: settle_invoice uses calculate_profit (or FeeManager::calculate_platform_fee)
// correctly: profit is split to platform/treasury (fee) and remainder to investor.
// These tests verify amounts with get_platform_fee_config and balance deltas.

/// settle_invoice uses calculate_profit (or FeeManager) correctly: investor receives
/// (payment - platform_fee), platform/treasury receives platform_fee; amounts match
/// get_platform_fee_config when fee system is initialized.
#[test]
fn test_settle_invoice_profit_split_matches_calculate_profit_and_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&500u32); // 5%

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let invoice_amount = 1_000i128;
    let investment_amount = 900i128;
    let payment_amount = 1_000i128; // profit = 100

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        invoice_amount,
        investment_amount,
    );

    let config = client
        .get_platform_fee_config()
        .expect("Fee config should exist after initialize_fee_system");
    assert_eq!(config.fee_bps, 500, "Config should reflect 5% fee");

    let (expected_investor_return, expected_platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);
    assert_eq!(
        expected_investor_return + expected_platform_fee,
        payment_amount,
        "calculate_profit must satisfy no-dust invariant"
    );

    let token_client = token::Client::new(&env, &currency);
    let initial_investor = token_client.balance(&investor);
    let initial_contract = token_client.balance(&contract_id);

    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&business, &payment_amount);
    let expiration = env.ledger().sequence() + 1_000;
    token_client.approve(
        &business,
        &contract_id,
        &payment_amount,
        &expiration,
    );

    client.settle_invoice(&invoice_id, &payment_amount);

    let investor_received = token_client.balance(&investor) - initial_investor;
    let platform_received = token_client.balance(&contract_id) - initial_contract;

    assert_eq!(
        investor_received, expected_investor_return,
        "Investor must receive amount from calculate_profit"
    );
    assert_eq!(
        platform_received, expected_platform_fee,
        "Platform/treasury must receive fee from calculate_profit"
    );
}

/// With fee config set via get_platform_fee_config: settlement amounts match
/// (investor_return, platform_fee) derived from config.fee_bps.
#[test]
fn test_settle_invoice_verify_amounts_with_get_platform_fee_config() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, QuickLendXContract);
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&200u32); // 2%

    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);

    let investment_amount = 800i128;
    let payment_amount = 1_000i128;

    let invoice_id = setup_funded_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        1_000i128,
        investment_amount,
    );

    let config = client.get_platform_fee_config().unwrap();
    assert_eq!(config.fee_bps, 200);

    let (investor_return, platform_fee) =
        client.calculate_profit(&investment_amount, &payment_amount);
    assert_eq!(platform_fee, 4); // 2% of 200 profit
    assert_eq!(investor_return, 996);

    let token_client = token::Client::new(&env, &currency);
    let initial_investor = token_client.balance(&investor);
    let initial_platform = token_client.balance(&contract_id);

    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&business, &payment_amount);
    let exp = env.ledger().sequence() + 1_000;
    token_client.approve(&business, &contract_id, &payment_amount, &exp);

    client.settle_invoice(&invoice_id, &payment_amount);

    assert_eq!(token_client.balance(&investor) - initial_investor, investor_return);
    assert_eq!(token_client.balance(&contract_id) - initial_platform, platform_fee);
}
