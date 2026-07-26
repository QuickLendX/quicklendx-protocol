#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, Vec, String,
};

use crate::errors::QuickLendXError;
use crate::payments::{create_escrow, EscrowStatus, EscrowStorage};
use crate::storage::InvoiceStorage;
use crate::types::{Invoice, InvoiceStatus, InvoiceCategory};
use crate::QuickLendXContract;
use crate::QuickLendXContractClient;
use crate::admin::AdminStorage;

// ============================================================================
// Helpers
// ============================================================================

const SECONDS_PER_DAY: u64 = 86_400;

fn register_contract(env: &Env) -> Address {
    env.register(QuickLendXContract, ())
}

fn setup_token(
    env: &Env,
    investor: &Address,
    business: &Address,
    contract_id: &Address,
    balance: i128,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(investor, &balance);
    sac.mint(business, &balance);
    let expiry = env.ledger().sequence() + 100_000;
    tok.approve(investor, contract_id, &balance, &expiry);
    tok.approve(business, contract_id, &balance, &expiry);
    currency
}

fn invoice_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn setup_escrow(env: &Env) -> (Address, Address, Address, BytesN<32>, Address, QuickLendXContractClient<'static>) {
    let contract_id = register_contract(env);
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let investor = Address::generate(env);
    let business = Address::generate(env);
    let amount = 5_000i128;
    let currency = setup_token(env, &investor, &business, &contract_id, amount * 10);

    let id = invoice_id(env, 1);
    let due_date = env.ledger().timestamp() + SECONDS_PER_DAY * 30;

    let invoice = Invoice {
        id: id.clone(),
        business: business.clone(),
        amount,
        currency: currency.clone(),
        due_date,
        status: InvoiceStatus::Funded,
        created_at: env.ledger().timestamp(),
        description: String::from_str(env, "Test Invoice"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(env),
        category: InvoiceCategory::Services,
        tags: Vec::new(env),
        funded_amount: amount,
        funded_at: Some(env.ledger().timestamp()),
        investor: Some(investor.clone()),
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: Vec::new(env),
        dispute_status: crate::types::DisputeStatus::None,
        dispute: crate::types::Dispute {
            created_by: Address::generate(env),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            resolution: String::from_str(env, ""),
            resolved_by: Address::generate(env),
            resolved_at: 0,
            resolution_outcome: crate::types::DisputeResolution::None,
        },
        total_paid: 0,
        payment_history: Vec::new(env),
        origination_fee_bps: None,
    };
    InvoiceStorage::store(env, &invoice);

    create_escrow(env, &id, &investor, &business, amount, &currency);

    (admin, investor, business, id, currency, client)
}

#[test]
fn extend_escrow_expiry_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (admin, _investor, _business, inv_id, _currency, client) = setup_escrow(&env);

    let invoice = InvoiceStorage::get_invoice(&env, &inv_id).unwrap();
    let new_due_date = invoice.due_date + SECONDS_PER_DAY * 10;

    client.extend_escrow_expiry(&admin, &inv_id, &new_due_date);

    let updated_invoice = InvoiceStorage::get_invoice(&env, &inv_id).unwrap();
    assert_eq!(updated_invoice.due_date, new_due_date);
}

#[test]
#[should_panic(expected = "HostError: Error(Value, InvalidInput)")]
fn extend_escrow_expiry_not_admin() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (_admin, _investor, _business, inv_id, _currency, client) = setup_escrow(&env);
    let fake_admin = Address::generate(&env);

    let invoice = InvoiceStorage::get_invoice(&env, &inv_id).unwrap();
    let new_due_date = invoice.due_date + SECONDS_PER_DAY * 10;

    // This should panic with NotAdmin or unauthorized
    client.extend_escrow_expiry(&fake_admin, &inv_id, &new_due_date);
}

#[test]
#[should_panic(expected = "HostError: Error(Value, InvalidInput)")]
fn extend_escrow_expiry_only_once() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (admin, _investor, _business, inv_id, _currency, client) = setup_escrow(&env);

    let invoice = InvoiceStorage::get_invoice(&env, &inv_id).unwrap();
    let new_due_date = invoice.due_date + SECONDS_PER_DAY * 10;

    client.extend_escrow_expiry(&admin, &inv_id, &new_due_date);
    
    // Second extension should fail with OperationNotAllowed
    let newer_due_date = new_due_date + SECONDS_PER_DAY * 10;
    client.extend_escrow_expiry(&admin, &inv_id, &newer_due_date);
}

#[test]
#[should_panic(expected = "HostError: Error(Value, InvalidInput)")]
fn extend_escrow_expiry_invalid_due_date() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (admin, _investor, _business, inv_id, _currency, client) = setup_escrow(&env);

    let invoice = InvoiceStorage::get_invoice(&env, &inv_id).unwrap();
    let new_due_date = invoice.due_date - SECONDS_PER_DAY * 10; // Earlier date!

    // This should fail with InvoiceDueDateInvalid
    client.extend_escrow_expiry(&admin, &inv_id, &new_due_date);
}

#[test]
#[should_panic(expected = "HostError: Error(Value, InvalidInput)")]
fn extend_escrow_expiry_not_held() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (admin, _investor, _business, inv_id, _currency, client) = setup_escrow(&env);
    
    // Release the escrow so it's no longer Held
    crate::payments::release_escrow(&env, &inv_id);

    let invoice = InvoiceStorage::get_invoice(&env, &inv_id).unwrap();
    let new_due_date = invoice.due_date + SECONDS_PER_DAY * 10;

    // This should fail with InvalidStatus
    client.extend_escrow_expiry(&admin, &inv_id, &new_due_date);
}

#[test]
#[should_panic(expected = "HostError: Error(Value, InvalidInput)")]
fn extend_escrow_expiry_past_ledger_cap() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (admin, _investor, _business, inv_id, _currency, client) = setup_escrow(&env);

    // Default max_due_date_days is 365.  Setting a due date more than 365 days
    // from now must be rejected by the ledger-cap guard.
    let past_cap = env.ledger().timestamp() + SECONDS_PER_DAY * 366;
    client.extend_escrow_expiry(&admin, &inv_id, &past_cap);
}

#[test]
fn extend_escrow_expiry_at_ledger_cap_boundary() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let (admin, _investor, _business, inv_id, _currency, client) = setup_escrow(&env);

    // Exactly at the 365-day cap boundary should succeed.
    let at_cap = env.ledger().timestamp() + SECONDS_PER_DAY * 365;
    client.extend_escrow_expiry(&admin, &inv_id, &at_cap);

    let updated = InvoiceStorage::get_invoice(&env, &inv_id).unwrap();
    assert_eq!(updated.due_date, at_cap);
}
