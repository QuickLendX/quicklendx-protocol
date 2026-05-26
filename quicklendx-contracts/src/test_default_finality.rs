use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    (env, client, admin)
}

fn create_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "KYC data"));
    client.verify_business(admin, &business);
    business
}

fn create_verified_investor(
    env: &Env,
    client: &QuickLendXContractClient,
    _admin: &Address,
    limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "KYC data"));
    client.verify_investor(&investor, &limit);
    investor
}

fn create_funded_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> (BytesN<32>, Address, Address, i128, Address) {
    let business = create_verified_business(env, client, admin);
    let investor = create_verified_investor(env, client, admin, 50_000);
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let token_client = token::Client::new(env, &currency);

    client.add_currency(admin, &currency);

    let initial_balance = 50_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiry = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &client.address, &initial_balance, &expiry);
    token_client.approve(&investor, &client.address, &initial_balance, &expiry);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 1000));
    client.accept_bid(&invoice_id, &bid_id);

    (invoice_id, business, investor, amount, currency)
}

fn default_invoice(env: &Env, client: &QuickLendXContractClient, invoice_id: &BytesN<32>) {
    let invoice = client.get_invoice(invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + GRACE_PERIOD + 1);
    client.mark_invoice_defaulted(invoice_id, &Some(GRACE_PERIOD));
}

// ---------------------------------------------------------------------------
// 1. Defaulted invoice cannot be refunded
// ---------------------------------------------------------------------------
#[test]
fn test_defaulted_invoice_cannot_be_refunded() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    default_invoice(&env, &client, &invoice_id);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err());

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ---------------------------------------------------------------------------
// 2. Defaulted invoice cannot be settled or partially paid
// ---------------------------------------------------------------------------
#[test]
fn test_defaulted_invoice_cannot_be_settled() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, _investor, amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    default_invoice(&env, &client, &invoice_id);

    let settle_result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(settle_result.is_err());

    let partial_result = client.try_process_partial_payment(
        &invoice_id,
        &1_000,
        &String::from_str(&env, "txn-1"),
    );
    assert!(partial_result.is_err());

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ---------------------------------------------------------------------------
// 3. Refunded invoice cannot be defaulted
// ---------------------------------------------------------------------------
#[test]
fn test_refunded_invoice_cannot_be_defaulted() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    client.refund_escrow_funds(&invoice_id, &business);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );

    let invoice = client.get_invoice(&invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + GRACE_PERIOD + 1);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE_PERIOD));
    assert!(result.is_err());

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );
}

// ---------------------------------------------------------------------------
// 4. Paid invoice cannot be defaulted
// ---------------------------------------------------------------------------
#[test]
fn test_paid_invoice_cannot_be_defaulted() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, _investor, amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    client.settle_invoice(&invoice_id, &amount);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Paid
    );

    let invoice = client.get_invoice(&invoice_id);
    env.ledger()
        .set_timestamp(invoice.due_date + GRACE_PERIOD + 1);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE_PERIOD));
    assert!(result.is_err());

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Paid
    );
}

// ---------------------------------------------------------------------------
// 5. Default blocks subsequent settlement attempt
// ---------------------------------------------------------------------------
#[test]
fn test_default_blocks_subsequent_settlement_attempt() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, investor, amount, currency) =
        create_funded_invoice(&env, &client, &admin);

    let token_client = token::Client::new(&env, &currency);
    let investor_balance_before = token_client.balance(&investor);

    default_invoice(&env, &client, &invoice_id);

    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(result.is_err());

    let investor_balance_after = token_client.balance(&investor);
    assert_eq!(investor_balance_before, investor_balance_after);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ---------------------------------------------------------------------------
// 6. Default blocks subsequent partial payment
// ---------------------------------------------------------------------------
#[test]
fn test_default_blocks_subsequent_partial_payment() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    default_invoice(&env, &client, &invoice_id);

    let result = client.try_process_partial_payment(
        &invoice_id,
        &1_000,
        &String::from_str(&env, "txn-pp"),
    );
    assert!(result.is_err());

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 0);
}

// ---------------------------------------------------------------------------
// 7. Refund blocks subsequent settlement
// ---------------------------------------------------------------------------
#[test]
fn test_refund_blocks_subsequent_settlement() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    client.refund_escrow_funds(&invoice_id, &business);

    let result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(result.is_err());

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Refunded
    );
}

// ---------------------------------------------------------------------------
// 8. Double default via mark then expiration check
// ---------------------------------------------------------------------------
#[test]
fn test_double_default_via_mark_and_expiration_check() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    default_invoice(&env, &client, &invoice_id);
    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );

    let expiration_result = client.check_invoice_expiration(&invoice_id, &Some(GRACE_PERIOD));
    assert_eq!(expiration_result, false);

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ---------------------------------------------------------------------------
// 9. Concurrent default attempts are idempotent (guard fires first)
// ---------------------------------------------------------------------------
#[test]
fn test_concurrent_default_attempts_idempotent() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    default_invoice(&env, &client, &invoice_id);

    let result = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE_PERIOD));
    assert!(result.is_err());
    let err = result.err().unwrap();
    let contract_err = err.expect("expected contract error");
    assert_eq!(contract_err, QuickLendXError::DuplicateDefaultTransition);

    assert_eq!(
        client.get_invoice(&invoice_id).status,
        InvoiceStatus::Defaulted
    );
}

// ---------------------------------------------------------------------------
// 10. Transition guard survives multiple pathways
// ---------------------------------------------------------------------------
#[test]
fn test_transition_guard_survives_multiple_pathways() {
    let (env, client, admin) = setup();
    let (invoice_id, _business, _investor, _amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    default_invoice(&env, &client, &invoice_id);

    let mark_result = client.try_mark_invoice_defaulted(&invoice_id, &Some(GRACE_PERIOD));
    assert!(mark_result.is_err());
    let mark_err = mark_result.err().unwrap().expect("expected contract error");
    assert_eq!(mark_err, QuickLendXError::DuplicateDefaultTransition);

    let exp_result = client.check_invoice_expiration(&invoice_id, &Some(GRACE_PERIOD));
    assert_eq!(exp_result, false);

    let handle_result = client.try_handle_default(&invoice_id);
    assert!(handle_result.is_err());
    let handle_err = handle_result
        .err()
        .unwrap()
        .expect("expected contract error");
    assert_eq!(handle_err, QuickLendXError::DuplicateDefaultTransition);
}

// ---------------------------------------------------------------------------
// 11. Default then refund then settle — all blocked
// ---------------------------------------------------------------------------
#[test]
fn test_ordering_default_then_refund_then_settle_all_blocked() {
    let (env, client, admin) = setup();
    let (invoice_id, business, _investor, amount, _currency) =
        create_funded_invoice(&env, &client, &admin);

    default_invoice(&env, &client, &invoice_id);

    let refund_result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(refund_result.is_err());

    let settle_result = client.try_settle_invoice(&invoice_id, &amount);
    assert!(settle_result.is_err());

    let partial_result = client.try_process_partial_payment(
        &invoice_id,
        &1_000,
        &String::from_str(&env, "txn-ord"),
    );
    assert!(partial_result.is_err());

    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Defaulted);
    assert_eq!(invoice.total_paid, 0);
}
