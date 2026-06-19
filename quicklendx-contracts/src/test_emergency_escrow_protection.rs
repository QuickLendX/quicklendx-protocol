//! Regression tests for emergency-withdraw protection of live escrow balances.
//!
//! Emergency recovery may withdraw same-token surplus held by the contract, but
//! it must not make tokens committed to `Held` escrows unavailable for the normal
//! release or refund paths.

use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::payments::EscrowStatus;
use crate::storage::InvoiceStorage;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

const INITIAL_BALANCE: i128 = 500_000;
const ESCROW_AMOUNT: i128 = 100_000;
const SECOND_ESCROW_AMOUNT: i128 = 60_000;
const SAME_TOKEN_SURPLUS: i128 = 25_000;
const OTHER_TOKEN_SURPLUS: i128 = 40_000;
const LEDGER_TIMESTAMP: u64 = 1_000_000;

macro_rules! assert_contract_error {
    ($result:expr, $error:expr) => {{
        let result = $result;
        assert!(
            matches!(&result, Err(Ok(actual)) if *actual == $error),
            "expected {:?}, got {:?}",
            $error,
            result
        );
    }};
}

struct FundedEscrow {
    invoice_id: BytesN<32>,
    business: Address,
    investor: Address,
    currency: Address,
}

struct Fixture {
    env: Env,
    client: QuickLendXContractClient<'static>,
    contract_id: Address,
    admin: Address,
    escrow: FundedEscrow,
}

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(LEDGER_TIMESTAMP);

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);

    (env, client, contract_id, admin)
}

fn verified_business(env: &Env, client: &QuickLendXContractClient<'_>, admin: &Address) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn verified_investor(
    env: &Env,
    client: &QuickLendXContractClient<'_>,
    investment_limit: i128,
) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &investment_limit);
    investor
}

fn setup_token(
    env: &Env,
    approved_addresses: &[&Address],
    contract_id: &Address,
    contract_balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let allowance_expiration = env.ledger().sequence() + 100_000;

    for address in approved_addresses {
        sac_client.mint(address, &INITIAL_BALANCE);
        token_client.approve(
            address,
            contract_id,
            &INITIAL_BALANCE,
            &allowance_expiration,
        );
    }

    if contract_balance > 0 {
        sac_client.mint(contract_id, &contract_balance);
    }

    currency
}

fn upload_verified_invoice(
    env: &Env,
    client: &QuickLendXContractClient<'_>,
    business: &Address,
    currency: &Address,
    amount: i128,
    description: &str,
) -> BytesN<32> {
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.upload_invoice(
        business,
        &amount,
        currency,
        &due_date,
        &String::from_str(env, description),
        &InvoiceCategory::Technology,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    invoice_id
}

fn fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient<'_>,
    business: &Address,
    investor: &Address,
    currency: &Address,
    amount: i128,
    description: &str,
) -> FundedEscrow {
    let invoice_id = upload_verified_invoice(env, client, business, currency, amount, description);
    let bid_id = client.place_bid(investor, &invoice_id, &amount, &(amount + 100));
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    let invoice = client.get_invoice(&invoice_id);
    let escrow = client.get_escrow_details(&invoice_id);

    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(escrow.status, EscrowStatus::Held);
    assert_eq!(escrow.amount, amount);
    assert_eq!(&escrow.currency, currency);

    FundedEscrow {
        invoice_id,
        business: business.clone(),
        investor: investor.clone(),
        currency: currency.clone(),
    }
}

fn repair_reserve(
    client: &QuickLendXContractClient<'_>,
    admin: &Address,
    currency: &Address,
    expected_scanned: u32,
    expected_reindexed: u32,
) {
    let report = client.repair_held_escrow_reserve(admin, currency, &0u32, &100u32);
    assert_eq!(report.scanned, expected_scanned);
    assert_eq!(report.reindexed, expected_reindexed);
    assert_eq!(report.next_offset, expected_scanned);
}

fn build_fixture(same_token_surplus: i128) -> Fixture {
    let (env, client, contract_id, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let investor = verified_investor(&env, &client, INITIAL_BALANCE);
    let currency = setup_token(
        &env,
        &[&business, &investor],
        &contract_id,
        same_token_surplus,
    );
    let escrow = fund_invoice(
        &env,
        &client,
        &business,
        &investor,
        &currency,
        ESCROW_AMOUNT,
        "Emergency escrow protection invoice",
    );
    repair_reserve(&client, &admin, &currency, 1, 1);

    Fixture {
        env,
        client,
        contract_id,
        admin,
        escrow,
    }
}

fn advance_to_unlock(env: &Env, pending: &crate::emergency::PendingEmergencyWithdrawal) {
    env.ledger().set_timestamp(pending.unlock_at);
}

fn set_invoice_status_for_test(
    env: &Env,
    contract_id: &Address,
    client: &QuickLendXContractClient<'_>,
    invoice_id: &BytesN<32>,
    status: InvoiceStatus,
) {
    let mut invoice = client.get_invoice(invoice_id);
    invoice.status = status;
    env.as_contract(contract_id, || {
        InvoiceStorage::update_invoice(env, &invoice);
    });
}

fn remove_reserve_total_for_test(env: &Env, contract_id: &Address, currency: &Address) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .remove(&(symbol_short!("esc_res"), currency.clone()));
    });
}

fn write_legacy_reserve_amount_for_test(
    env: &Env,
    contract_id: &Address,
    currency: &Address,
    amount: i128,
) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&(symbol_short!("esc_res"), currency.clone()), &amount);
    });
}

fn remove_reserve_marker_for_test(env: &Env, contract_id: &Address, escrow_id: &BytesN<32>) {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .remove(&(symbol_short!("esc_acc"), escrow_id.clone()));
    });
}

fn remove_reserve_sidecar_for_test(
    env: &Env,
    contract_id: &Address,
    currency: &Address,
    escrow_id: &BytesN<32>,
) {
    remove_reserve_total_for_test(env, contract_id, currency);
    remove_reserve_marker_for_test(env, contract_id, escrow_id);
}

#[test]
fn same_token_emergency_withdraw_rejects_live_escrow_balance() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);
    let blocked_amount = SAME_TOKEN_SURPLUS + 1;

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &blocked_amount,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    let result = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        result,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    assert_eq!(token_client.balance(&target), 0);
    assert_eq!(
        token_client.balance(&fixture.contract_id),
        ESCROW_AMOUNT + SAME_TOKEN_SURPLUS
    );
    assert!(fixture.client.get_pending_emergency_withdraw().is_some());
    assert_eq!(
        fixture
            .client
            .get_escrow_details(&fixture.escrow.invoice_id)
            .status,
        EscrowStatus::Held
    );

    let investor_before = token_client.balance(&fixture.escrow.investor);
    fixture
        .client
        .refund_escrow_funds(&fixture.escrow.invoice_id, &fixture.escrow.business);
    assert_eq!(
        token_client.balance(&fixture.escrow.investor),
        investor_before + ESCROW_AMOUNT
    );
}

#[test]
fn emergency_withdraw_rejects_when_balance_is_below_held_reserve() {
    let fixture = build_fixture(0);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let sac_client = token::StellarAssetClient::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);
    let burn_amount = ESCROW_AMOUNT / 2;

    sac_client.burn(&fixture.contract_id, &burn_amount);
    assert_eq!(
        token_client.balance(&fixture.contract_id),
        ESCROW_AMOUNT - burn_amount
    );

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &1,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    let result = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        result,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    assert_eq!(token_client.balance(&target), 0);
    assert_eq!(
        token_client.balance(&fixture.contract_id),
        ESCROW_AMOUNT - burn_amount
    );
    assert_eq!(
        fixture
            .client
            .get_escrow_details(&fixture.escrow.invoice_id)
            .status,
        EscrowStatus::Held
    );
}

#[test]
fn missing_reserve_sidecar_is_repaired_by_paginated_admin_repair() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);
    let escrow = fixture
        .client
        .get_escrow_details(&fixture.escrow.invoice_id);

    remove_reserve_sidecar_for_test(
        &fixture.env,
        &fixture.contract_id,
        &fixture.escrow.currency,
        &escrow.escrow_id,
    );

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &SAME_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    let before_repair = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        before_repair,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );
    assert!(!fixture.client.can_exec_emergency());
    assert_eq!(token_client.balance(&target), 0);
    assert_eq!(
        token_client.balance(&fixture.contract_id),
        ESCROW_AMOUNT + SAME_TOKEN_SURPLUS
    );

    repair_reserve(
        &fixture.client,
        &fixture.admin,
        &fixture.escrow.currency,
        1,
        1,
    );

    assert!(fixture.client.can_exec_emergency());
    fixture.client.execute_emergency_withdraw(&fixture.admin);
    assert_eq!(token_client.balance(&target), SAME_TOKEN_SURPLUS);
    assert_eq!(token_client.balance(&fixture.contract_id), ESCROW_AMOUNT);

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &1,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);
    let no_surplus_left = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        no_surplus_left,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    let investor_before = token_client.balance(&fixture.escrow.investor);
    fixture
        .client
        .refund_escrow_funds(&fixture.escrow.invoice_id, &fixture.escrow.business);
    assert_eq!(
        token_client.balance(&fixture.escrow.investor),
        investor_before + ESCROW_AMOUNT
    );
    assert_eq!(token_client.balance(&fixture.contract_id), 0);
}

#[test]
fn reserve_repair_recomputes_exact_total_when_marker_is_missing() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);
    let escrow = fixture
        .client
        .get_escrow_details(&fixture.escrow.invoice_id);

    remove_reserve_marker_for_test(&fixture.env, &fixture.contract_id, &escrow.escrow_id);
    repair_reserve(
        &fixture.client,
        &fixture.admin,
        &fixture.escrow.currency,
        1,
        1,
    );

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &SAME_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);
    fixture.client.execute_emergency_withdraw(&fixture.admin);

    assert_eq!(token_client.balance(&target), SAME_TOKEN_SURPLUS);
    assert_eq!(token_client.balance(&fixture.contract_id), ESCROW_AMOUNT);
}

#[test]
fn reserve_repair_handles_missing_total_with_existing_marker() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);

    remove_reserve_total_for_test(&fixture.env, &fixture.contract_id, &fixture.escrow.currency);

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &(SAME_TOKEN_SURPLUS + 1),
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    let before_repair = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        before_repair,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    repair_reserve(
        &fixture.client,
        &fixture.admin,
        &fixture.escrow.currency,
        1,
        1,
    );

    let after_repair = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        after_repair,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &SAME_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);
    fixture.client.execute_emergency_withdraw(&fixture.admin);

    assert_eq!(token_client.balance(&target), SAME_TOKEN_SURPLUS);
    assert_eq!(token_client.balance(&fixture.contract_id), ESCROW_AMOUNT);
}

#[test]
fn legacy_amount_only_reserve_is_incomplete_until_repaired() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);

    write_legacy_reserve_amount_for_test(
        &fixture.env,
        &fixture.contract_id,
        &fixture.escrow.currency,
        ESCROW_AMOUNT,
    );

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &SAME_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    let before_repair = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        before_repair,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    repair_reserve(
        &fixture.client,
        &fixture.admin,
        &fixture.escrow.currency,
        1,
        1,
    );

    fixture.client.execute_emergency_withdraw(&fixture.admin);
    assert_eq!(token_client.balance(&target), SAME_TOKEN_SURPLUS);
    assert_eq!(token_client.balance(&fixture.contract_id), ESCROW_AMOUNT);
}

#[test]
fn readiness_query_requires_non_escrow_surplus() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let target = Address::generate(&fixture.env);

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &(SAME_TOKEN_SURPLUS + 1),
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    assert!(!fixture.client.can_exec_emergency());

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &SAME_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    assert!(fixture.client.can_exec_emergency());
}

#[test]
fn same_token_emergency_withdraw_allows_only_non_escrow_surplus() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &SAME_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    fixture.client.execute_emergency_withdraw(&fixture.admin);

    assert_eq!(token_client.balance(&target), SAME_TOKEN_SURPLUS);
    assert_eq!(token_client.balance(&fixture.contract_id), ESCROW_AMOUNT);
    assert!(fixture.client.get_pending_emergency_withdraw().is_none());
    assert_eq!(
        fixture
            .client
            .get_escrow_details(&fixture.escrow.invoice_id)
            .status,
        EscrowStatus::Held
    );

    let investor_before = token_client.balance(&fixture.escrow.investor);
    fixture
        .client
        .refund_escrow_funds(&fixture.escrow.invoice_id, &fixture.escrow.business);
    assert_eq!(
        token_client.balance(&fixture.escrow.investor),
        investor_before + ESCROW_AMOUNT
    );
    assert_eq!(token_client.balance(&fixture.contract_id), 0);
}

#[test]
fn emergency_withdraw_of_different_token_ignores_held_escrow_currency() {
    let fixture = build_fixture(0);
    let other_token = setup_token(&fixture.env, &[], &fixture.contract_id, OTHER_TOKEN_SURPLUS);
    let escrow_token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let other_token_client = token::Client::new(&fixture.env, &other_token);
    let target = Address::generate(&fixture.env);

    repair_reserve(&fixture.client, &fixture.admin, &other_token, 1, 0);

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &other_token,
        &OTHER_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    fixture.client.execute_emergency_withdraw(&fixture.admin);

    assert_eq!(other_token_client.balance(&target), OTHER_TOKEN_SURPLUS);
    assert_eq!(other_token_client.balance(&fixture.contract_id), 0);
    assert_eq!(
        escrow_token_client.balance(&fixture.contract_id),
        ESCROW_AMOUNT
    );
    assert_eq!(
        fixture
            .client
            .get_escrow_details(&fixture.escrow.invoice_id)
            .status,
        EscrowStatus::Held
    );
}

#[test]
fn multiple_same_token_held_escrows_are_reserved_together() {
    let (env, client, contract_id, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let investor_a = verified_investor(&env, &client, INITIAL_BALANCE);
    let investor_b = verified_investor(&env, &client, INITIAL_BALANCE);
    let currency = setup_token(
        &env,
        &[&business, &investor_a, &investor_b],
        &contract_id,
        SAME_TOKEN_SURPLUS,
    );
    let escrow_a = fund_invoice(
        &env,
        &client,
        &business,
        &investor_a,
        &currency,
        ESCROW_AMOUNT,
        "First live escrow",
    );
    let escrow_b = fund_invoice(
        &env,
        &client,
        &business,
        &investor_b,
        &currency,
        SECOND_ESCROW_AMOUNT,
        "Second live escrow",
    );
    repair_reserve(&client, &admin, &currency, 2, 2);
    let token_client = token::Client::new(&env, &currency);
    let target = Address::generate(&env);

    client.initiate_emergency_withdraw(&admin, &currency, &(SAME_TOKEN_SURPLUS + 1), &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&env, &pending);

    let result = client.try_execute_emergency_withdraw(&admin);
    assert_contract_error!(
        result,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    assert_eq!(token_client.balance(&target), 0);
    assert_eq!(
        token_client.balance(&contract_id),
        ESCROW_AMOUNT + SECOND_ESCROW_AMOUNT + SAME_TOKEN_SURPLUS
    );
    assert_eq!(
        client.get_escrow_details(&escrow_a.invoice_id).status,
        EscrowStatus::Held
    );
    assert_eq!(
        client.get_escrow_details(&escrow_b.invoice_id).status,
        EscrowStatus::Held
    );

    client.initiate_emergency_withdraw(&admin, &currency, &SAME_TOKEN_SURPLUS, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&env, &pending);

    client.execute_emergency_withdraw(&admin);

    assert_eq!(token_client.balance(&target), SAME_TOKEN_SURPLUS);
    assert_eq!(
        token_client.balance(&contract_id),
        ESCROW_AMOUNT + SECOND_ESCROW_AMOUNT
    );
    assert_eq!(
        client.get_escrow_details(&escrow_a.invoice_id).status,
        EscrowStatus::Held
    );
    assert_eq!(
        client.get_escrow_details(&escrow_b.invoice_id).status,
        EscrowStatus::Held
    );
}

#[test]
fn incomplete_paginated_repair_keeps_emergency_withdraw_closed() {
    let (env, client, contract_id, admin) = setup();
    let business = verified_business(&env, &client, &admin);
    let investor_a = verified_investor(&env, &client, INITIAL_BALANCE);
    let investor_b = verified_investor(&env, &client, INITIAL_BALANCE);
    let currency = setup_token(
        &env,
        &[&business, &investor_a, &investor_b],
        &contract_id,
        SAME_TOKEN_SURPLUS,
    );
    let escrow_a = fund_invoice(
        &env,
        &client,
        &business,
        &investor_a,
        &currency,
        ESCROW_AMOUNT,
        "First paginated repair escrow",
    );
    let escrow_b = fund_invoice(
        &env,
        &client,
        &business,
        &investor_b,
        &currency,
        SECOND_ESCROW_AMOUNT,
        "Second paginated repair escrow",
    );
    let target = Address::generate(&env);
    let token_client = token::Client::new(&env, &currency);
    let blocked_amount = 10_000i128;
    let blocked_invoice = upload_verified_invoice(
        &env,
        &client,
        &business,
        &currency,
        blocked_amount,
        "Existing invoice blocked during active paginated repair",
    );
    let blocked_bid = client.place_bid(
        &investor_b,
        &blocked_invoice,
        &blocked_amount,
        &(blocked_amount + 100),
    );

    let out_of_order = client.try_repair_held_escrow_reserve(&admin, &currency, &1u32, &1u32);
    assert_contract_error!(out_of_order, QuickLendXError::InvalidStatus);

    let first_page = client.repair_held_escrow_reserve(&admin, &currency, &0u32, &1u32);
    assert_eq!(first_page.scanned, 1);
    assert_eq!(first_page.reindexed, 0);
    assert_eq!(first_page.next_offset, 1);

    client.initiate_emergency_withdraw(&admin, &currency, &SAME_TOKEN_SURPLUS, &target);
    let pending = client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&env, &pending);
    let incomplete = client.try_execute_emergency_withdraw(&admin);
    assert_contract_error!(
        incomplete,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    let blocked_release = client.try_release_escrow_funds(&escrow_a.invoice_id);
    assert_contract_error!(blocked_release, QuickLendXError::InvalidStatus);

    let blocked_refund = client.try_refund_escrow_funds(&escrow_b.invoice_id, &admin);
    assert_contract_error!(blocked_refund, QuickLendXError::InvalidStatus);

    let blocked_upload_due_date = env.ledger().timestamp() + 86_400;
    let blocked_upload = client.try_upload_invoice(
        &business,
        &blocked_amount,
        &currency,
        &blocked_upload_due_date,
        &String::from_str(&env, "Upload blocked during active paginated repair"),
        &InvoiceCategory::Technology,
        &Vec::new(&env),
    );
    assert_contract_error!(blocked_upload, QuickLendXError::InvalidStatus);

    let blocked_create = client.try_accept_bid_and_fund(&blocked_invoice, &blocked_bid);
    assert_contract_error!(blocked_create, QuickLendXError::InvalidStatus);
    assert_eq!(
        token_client.balance(&contract_id),
        ESCROW_AMOUNT + SECOND_ESCROW_AMOUNT + SAME_TOKEN_SURPLUS
    );

    let second_page = client.repair_held_escrow_reserve(&admin, &currency, &1u32, &1u32);
    assert_eq!(second_page.scanned, 1);
    assert_eq!(second_page.reindexed, 1);
    assert_eq!(second_page.next_offset, 2);

    let still_incomplete = client.try_execute_emergency_withdraw(&admin);
    assert_contract_error!(
        still_incomplete,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    let final_page = client.repair_held_escrow_reserve(&admin, &currency, &2u32, &1u32);
    assert_eq!(final_page.scanned, 1);
    assert_eq!(final_page.reindexed, 1);
    assert_eq!(final_page.next_offset, 3);
    assert!(client.can_exec_emergency());
}

#[test]
fn held_escrow_remains_reserved_after_invoice_leaves_funded_status() {
    let fixture = build_fixture(0);
    let token_client = token::Client::new(&fixture.env, &fixture.escrow.currency);
    let target = Address::generate(&fixture.env);

    set_invoice_status_for_test(
        &fixture.env,
        &fixture.contract_id,
        &fixture.client,
        &fixture.escrow.invoice_id,
        InvoiceStatus::Defaulted,
    );
    assert_eq!(
        fixture
            .client
            .get_invoice(&fixture.escrow.invoice_id)
            .status,
        InvoiceStatus::Defaulted
    );

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &1,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();
    advance_to_unlock(&fixture.env, &pending);

    let result = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        result,
        QuickLendXError::EmergencyWithdrawInsufficientBalance
    );

    assert_eq!(token_client.balance(&target), 0);
    assert_eq!(token_client.balance(&fixture.contract_id), ESCROW_AMOUNT);
    assert_eq!(
        fixture
            .client
            .get_escrow_details(&fixture.escrow.invoice_id)
            .status,
        EscrowStatus::Held
    );
}

#[test]
fn timelock_and_expiration_still_gate_surplus_with_live_escrow() {
    let fixture = build_fixture(SAME_TOKEN_SURPLUS);
    let target = Address::generate(&fixture.env);

    fixture.client.initiate_emergency_withdraw(
        &fixture.admin,
        &fixture.escrow.currency,
        &SAME_TOKEN_SURPLUS,
        &target,
    );
    let pending = fixture.client.get_pending_emergency_withdraw().unwrap();

    fixture.env.ledger().set_timestamp(pending.unlock_at - 1);
    let before_unlock = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(
        before_unlock,
        QuickLendXError::EmergencyWithdrawTimelockNotElapsed
    );

    fixture.env.ledger().set_timestamp(pending.expires_at);
    let at_expiration = fixture
        .client
        .try_execute_emergency_withdraw(&fixture.admin);
    assert_contract_error!(at_expiration, QuickLendXError::EmergencyWithdrawExpired);

    assert!(fixture.client.get_pending_emergency_withdraw().is_some());
    assert_eq!(
        fixture
            .client
            .get_escrow_details(&fixture.escrow.invoice_id)
            .status,
        EscrowStatus::Held
    );
}
