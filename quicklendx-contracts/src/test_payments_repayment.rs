/// Integration tests for the deterministic escrow repayment distribution
/// (`repay_escrow`): principal release, investor return, platform fee, and the
/// no-partial-state guarantees on failure / replay.
use super::*;
use crate::errors::QuickLendXError;
use crate::escrow::EscrowStatus;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
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
    let initial_balance = 1_000_000i128;
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

fn create_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, "Test Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

/// Fund an invoice and return `(invoice_id, business, investor, currency)`.
fn fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    amount: i128,
) -> (BytesN<32>, Address, Address, Address) {
    let contract_id = client.address.clone();
    let business = setup_verified_business(env, client, admin);
    let investor = setup_verified_investor(env, client, amount * 10);
    let currency = setup_token(env, &business, &investor, &contract_id);
    let invoice_id = create_verified_invoice(env, client, &business, amount, &currency);

    // Place a bid that fully funds the invoice, then accept it (Held escrow).
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &amount,
        &(amount + 1000),
        &BytesN::from_array(env, &[0u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Funded
    );
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
    (invoice_id, business, investor, currency)
}

#[test]
fn test_repay_escrow_distributes_exactly() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let payment_amount = 11_000i128; // 1000 profit
    let (invoice_id, business, investor, currency) = fund_invoice(&env, &client, &admin, amount);

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    let investor_before = token_client.balance(&investor);
    let business_before = token_client.balance(&business);
    let contract_before = token_client.balance(&contract_id);

    // Custody the business repayment inside the contract.
    sac_client.mint(&contract_id, &payment_amount);
    assert_eq!(
        token_client.balance(&contract_id),
        contract_before + payment_amount
    );

    let result = client.try_repay_escrow(&invoice_id, &payment_amount, &0);
    assert!(result.is_ok(), "repay_escrow call must succeed");
    let alloc = result.unwrap();
    assert_eq!(alloc.principal_return, 10_000);
    assert_eq!(alloc.platform_fee, 20);
    assert_eq!(alloc.investor_return, 10_980);
    assert_eq!(
        alloc.investor_return + alloc.platform_fee + alloc.late_fee,
        payment_amount
    );

    // Investor received the return; business received the released principal.
    assert_eq!(token_client.balance(&investor), investor_before + 10_980);
    assert_eq!(token_client.balance(&business), business_before + 10_000);

    // Escrow is now released (terminal) and cannot be repaid again.
    assert_eq!(
        client.get_escrow_status(&invoice_id),
        EscrowStatus::Released
    );
    let replay = client.try_repay_escrow(&invoice_id, &payment_amount, &0);
    assert_eq!(replay, Err(Ok(QuickLendXError::InvalidStatus)));
}

#[test]
fn test_repay_escrow_insufficient_custody_leaves_no_partial_state() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let payment_amount = 11_000i128;
    let (invoice_id, business, investor, currency) = fund_invoice(&env, &client, &admin, amount);

    let token_client = token::Client::new(&env, &currency);

    let investor_before = token_client.balance(&investor);
    let business_before = token_client.balance(&business);
    let contract_before = token_client.balance(&contract_id);

    // Only partially custody the repayment (short by 1) -> must fail.
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&contract_id, &(payment_amount - 1));

    let result = client.try_repay_escrow(&invoice_id, &payment_amount, &0);
    assert_eq!(result, Err(Ok(QuickLendXError::InsufficientFunds)));

    // No funds moved: investor, business, and contract balances unchanged, escrow Held.
    assert_eq!(token_client.balance(&investor), investor_before);
    assert_eq!(token_client.balance(&business), business_before);
    assert_eq!(
        token_client.balance(&contract_id),
        contract_before + (payment_amount - 1)
    );
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

#[test]
fn test_repay_escrow_overcharge_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let (invoice_id, _business, _investor, currency) = fund_invoice(&env, &client, &admin, amount);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    // payment 10 -> cannot cover a 100% late fee on the 10000 principal.
    sac_client.mint(&contract_id, &10);
    let result = client.try_repay_escrow(&invoice_id, &10, &10_000);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAmount)));
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Held);
}

// ============================================================================
// Idempotency tests for repay_escrow_with_key
// ============================================================================

fn request_key(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn test_keyed_repay_safe_retry_is_idempotent() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let payment_amount = 11_000i128;
    let (invoice_id, _business, _investor, currency) = fund_invoice(&env, &client, &admin, amount);

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&contract_id, &payment_amount);

    let key = request_key(&env, 1);

    let first = client
        .try_repay_escrow_with_key(&invoice_id, &payment_amount, &0, &key)
        .expect("host ok")
        .expect("first repayment succeeds");
    let second = client
        .try_repay_escrow_with_key(&invoice_id, &payment_amount, &0, &key)
        .expect("host ok")
        .expect("safe retry succeeds");

    assert_eq!(first.principal_return, second.principal_return);
    assert_eq!(first.investor_return, second.investor_return);
    assert_eq!(first.platform_fee, second.platform_fee);
    assert_eq!(
        token_client.balance(&contract_id),
        amount + payment_amount - first.investor_return - first.platform_fee
    );
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Released);
}

#[test]
fn test_keyed_repay_conflicting_reuse_rejected() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let (invoice_id, _business, _investor, currency) = fund_invoice(&env, &client, &admin, amount);

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&contract_id, &11_000);

    let key = request_key(&env, 2);
    let _ = client
        .try_repay_escrow_with_key(&invoice_id, &11_000, &0, &key)
        .expect("host ok")
        .expect("first repayment succeeds");

    let conflict = client.try_repay_escrow_with_key(&invoice_id, &12_000, &0, &key);
    assert!(conflict.is_err(), "conflicting reuse must be rejected");
    let err = conflict.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::DuplicateBid);
    assert_eq!(
        token_client.balance(&contract_id),
        amount + 11_000 - 10_980 - 20
    );
}

#[test]
fn test_keyed_repay_rejected_attempt_does_not_poison_key() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let payment_amount = 11_000i128;
    let (invoice_id, _business, _investor, currency) = fund_invoice(&env, &client, &admin, amount);

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);

    // Only partially custody the repayment (short by 1) -> must fail.
    sac_client.mint(&contract_id, &(payment_amount - 1));

    let key = request_key(&env, 3);
    let first = client.try_repay_escrow_with_key(&invoice_id, &payment_amount, &0, &key);
    assert_eq!(first, Err(Ok(QuickLendXError::InsufficientFunds)));

    // No record stored; corrected retry with full custody must succeed.
    sac_client.mint(&contract_id, &1);
    let corrected = client
        .try_repay_escrow_with_key(&invoice_id, &payment_amount, &0, &key)
        .expect("host ok")
        .expect("corrected retry succeeds");
    assert_eq!(corrected.investor_return, 10_980);
    assert_eq!(client.get_escrow_status(&invoice_id), EscrowStatus::Released);
}

#[test]
fn test_keyed_repay_timeout_retry_same_result_across_ledgers() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let payment_amount = 11_000i128;
    let (invoice_id, _business, _investor, currency) = fund_invoice(&env, &client, &admin, amount);

    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    sac_client.mint(&contract_id, &payment_amount);

    let key = request_key(&env, 4);
    let first = client
        .try_repay_escrow_with_key(&invoice_id, &payment_amount, &0, &key)
        .expect("host ok")
        .expect("first repayment succeeds");

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 86_400 * 5);

    let second = client
        .try_repay_escrow_with_key(&invoice_id, &payment_amount, &0, &key)
        .expect("host ok")
        .expect("timeout retry succeeds");

    assert_eq!(first.investor_return, second.investor_return);
    assert_eq!(first.platform_fee, second.platform_fee);
    assert_eq!(
        token_client.balance(&contract_id),
        amount + payment_amount - first.investor_return - first.platform_fee
    );
}
