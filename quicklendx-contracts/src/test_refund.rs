/// Test suite for escrow refund flow
///
/// Test Coverage:
/// 1. Authorization: Only admin or business owner can trigger a refund
/// 2. State Validation: Only funded invoices can be refunded
/// 3. Token Transfer: Funds are returned to the correct investor
/// 4. State Transitions: Invoice, Bid, Investment, and Escrow statuses are correctly updated
/// 5. Security: Unauthorized callers cannot trigger refunds
use super::*;
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStatus;
#[cfg(test)]
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

// ============================================================================
// Helper Functions (Reused from test_escrow.rs pattern)
// ============================================================================

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();

    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);

    let initial_balance = 100_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);

    currency
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (BytesN<32>, Address, Address, i128, Address) {
    let business = setup_verified_business(env, client, admin);
    let investor = setup_verified_investor(env, client, 50_000);
    let currency = setup_token(env, &business, &investor, &client.address);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1000));
    client.accept_bid(&invoice_id, &bid_id);

    (invoice_id, business, investor, amount, currency)
}

// ============================================================================
// Test Cases
// ============================================================================

#[test]
fn test_business_can_trigger_refund() {
    let (env, client, admin) = setup();
    let (invoice_id, business, investor, amount, currency) =
        create_funded_invoice(&env, &client, &admin);
    let token_client = token::Client::new(&env, &currency);

    let investor_balance_before = token_client.balance(&investor);

    // Business owner triggers refund
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(
        result.is_ok(),
        "Business owner should be able to trigger refund"
    );

    // Verify investor received funds back
    let investor_balance_after = token_client.balance(&investor);
    assert_eq!(investor_balance_after - investor_balance_before, amount);

    // Verify state transitions
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);

    let escrow = client.get_escrow_details(&invoice_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);

    env.as_contract(&client.address, || {
        let investment =
            crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
                .unwrap();
        assert_eq!(investment.status, InvestmentStatus::Refunded);
    });

    let bid = client.get_bids_for_invoice(&invoice_id).get(0).unwrap();
    assert_eq!(bid.status, BidStatus::Cancelled);
}

#[test]
fn test_admin_can_trigger_refund() {
    let (env, client, admin) = setup();
    let (invoice_id, _, investor, amount, currency) = create_funded_invoice(&env, &client, &admin);
    let token_client = token::Client::new(&env, &currency);

    let investor_balance_before = token_client.balance(&investor);

    // Admin triggers refund
    let result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(result.is_ok(), "Admin should be able to trigger refund");

    // Verify investor received funds back
    let investor_balance_after = token_client.balance(&investor);
    assert_eq!(investor_balance_after - investor_balance_before, amount);

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Refunded);
}

#[test]
fn test_unauthorized_caller_cannot_trigger_refund() {
    let (env, client, admin) = setup();
    let (invoice_id, _, _, _, _) = create_funded_invoice(&env, &client, &admin);
    let stranger = Address::generate(&env);

    // Stranger tries to trigger refund
    let result = client.try_refund_escrow_funds(&invoice_id, &stranger);
    assert!(
        result.is_err(),
        "Stranger should not be able to trigger refund"
    );

    // Verify invoice is still Funded
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}

#[test]
fn test_cannot_refund_unfunded_invoice() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Invoice is Verified but not Funded
    let result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(result.is_err(), "Cannot refund an unfunded invoice");
}

#[test]
fn test_cannot_refund_twice() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _, _, _) = create_funded_invoice(&env, &client, &admin);

    // First refund
    client.refund_escrow_funds(&invoice_id, &business);

    // Second refund attempt
    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err(), "Cannot refund an already refunded invoice");
}

#[test]
fn test_cannot_refund_nonexistent_invoice() {
    let (env, client, admin) = setup();

    // Generate a random invoice ID that doesn't exist
    let nonexistent_invoice_id = BytesN::from_array(&env, &[1u8; 32]);

    // Attempt to refund
    let result = client.try_refund_escrow_funds(&nonexistent_invoice_id, &admin);

    // Verify it returns an error
    assert!(result.is_err(), "Cannot refund a nonexistent invoice");
}

#[test]
fn test_cannot_refund_missing_escrow() {
    let (env, client, admin) = setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &business, &investor, &client.address);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;

    // Create and verify an invoice
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Test Missing Escrow"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    // Forcibly update status to Funded, skipping the bid process (no escrow record created)
    client.update_invoice_status(&invoice_id, &InvoiceStatus::Funded);

    // Attempt to refund should fail because there is no corresponding escrow record
    let result = client.try_refund_escrow_funds(&invoice_id, &admin);
    assert!(
        result.is_err(),
        "Cannot refund an invoice if the escrow record is missing"
    );
}

#[test]
fn test_refund_updates_internal_states_correctly() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    // Pre-refund state verification
    let pre_refund_invoice = client.get_invoice(&invoice_id);
    assert_eq!(pre_refund_invoice.status, InvoiceStatus::Funded);

    // Status list tracking count check before refund
    let pre_refunded_count = client.get_invoice_count_by_status(&InvoiceStatus::Refunded);

    // Perform the refund
    client.refund_escrow_funds(&invoice_id, &business);

    // 1. Invoice Status should update to Refunded
    let post_refund_invoice = client.get_invoice(&invoice_id);
    assert_eq!(post_refund_invoice.status, InvoiceStatus::Refunded);

    // 2. Invoice Status tracking lists should be updated correctly
    let post_refunded_count = client.get_invoice_count_by_status(&InvoiceStatus::Refunded);

    assert_eq!(post_refunded_count, pre_refunded_count + 1);

    // 3. Bid status should update to Cancelled
    let bids = client.get_bids_for_invoice(&invoice_id);
    assert_eq!(bids.len(), 1);
    assert_eq!(bids.get(0).unwrap().status, BidStatus::Cancelled);

    // 4. Investment status should update to Refunded
    env.as_contract(&client.address, || {
        let investment =
            crate::investment::InvestmentStorage::get_investment_by_invoice(&env, &invoice_id)
                .unwrap();
        assert_eq!(investment.status, InvestmentStatus::Refunded);
    });
}
