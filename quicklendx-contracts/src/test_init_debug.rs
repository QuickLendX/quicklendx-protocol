#![cfg(test)]

use super::*;
use crate::admin::require_not_reserved;
use crate::init::InitializationParams;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn base_params(
    admin: Address,
    treasury: Address,
    currencies: Vec<Address>,
) -> InitializationParams {
    InitializationParams {
        admin,
        treasury,
        fee_bps: 200,
        min_invoice_amount: 1_000_000,
        max_due_date_days: 365,
        grace_period_seconds: 604800,
        initial_currencies: currencies,
        corridors: Vec::new(&env),
        backfill_max_batch_size: 100,
    }
}

// ============================================================================
// Tests for require_not_reserved (reserved address list membership)
// ============================================================================

fn setup_initialized() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let contract_addr = contract_id.clone();
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let params = base_params(admin.clone(), treasury.clone(), Vec::new(&env));
    client.initialize(&params);
    (env, admin, treasury, contract_addr)
}

fn call_require_not_reserved(
    env: &Env,
    contract_id: &Address,
    address: &Address,
    admin: Option<Address>,
    treasury: Option<Address>,
    contract_address: Option<Address>,
) -> Result<(), QuickLendXError> {
    env.as_contract(contract_id, || {
        require_not_reserved(env, address, admin, treasury, contract_address)
    })
}

#[test]
fn test_require_not_reserved_rejects_zero_address() {
    let (env, admin, treasury, contract_addr) = setup_initialized();
    let zero_addr = Address::from_string(&String::from_str(
        &env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    let contract_id = contract_addr.clone();
    let err = call_require_not_reserved(
        &env,
        &contract_id,
        &zero_addr,
        Some(admin),
        Some(treasury),
        Some(contract_addr),
    );
    assert_eq!(err, Err(QuickLendXError::InvalidCurrency));
}

#[test]
fn test_require_not_reserved_rejects_contract_address() {
    let (env, admin, treasury, contract_addr) = setup_initialized();
    let contract_id = contract_addr.clone();
    let err = call_require_not_reserved(
        &env,
        &contract_id,
        &contract_addr.clone(),
        Some(admin),
        Some(treasury),
        Some(contract_addr),
    );
    assert_eq!(err, Err(QuickLendXError::InvalidCurrency));
}

#[test]
fn test_require_not_reserved_rejects_admin_address() {
    let (env, admin, treasury, contract_addr) = setup_initialized();
    let contract_id = contract_addr.clone();
    let err = call_require_not_reserved(
        &env,
        &contract_id,
        &admin,
        Some(admin.clone()),
        Some(treasury),
        Some(contract_addr),
    );
    assert_eq!(err, Err(QuickLendXError::InvalidCurrency));
}

#[test]
fn test_require_not_reserved_rejects_treasury_address() {
    let (env, admin, treasury, contract_addr) = setup_initialized();
    let contract_id = contract_addr.clone();
    let err = call_require_not_reserved(
        &env,
        &contract_id,
        &treasury,
        Some(admin),
        Some(treasury.clone()),
        Some(contract_addr),
    );
    assert_eq!(err, Err(QuickLendXError::InvalidCurrency));
}

#[test]
fn test_require_not_reserved_allows_regular_address() {
    let (env, admin, treasury, contract_addr) = setup_initialized();
    let regular = Address::generate(&env);
    let contract_id = contract_addr.clone();
    let result = call_require_not_reserved(
        &env,
        &contract_id,
        &regular,
        Some(admin),
        Some(treasury),
        Some(contract_addr),
    );
    assert_eq!(result, Ok(()));
}

#[test]
fn test_require_not_reserved_allows_regular_without_explicit_admin() {
    let (env, admin, treasury, contract_addr) = setup_initialized();
    let regular = Address::generate(&env);
    // Pass None for admin/treasury - should fetch from storage
    let contract_id = contract_addr.clone();
    let result = call_require_not_reserved(
        &env,
        &contract_id,
        &regular,
        None,
        None,
        Some(contract_addr),
    );
    assert_eq!(result, Ok(()));
}

#[test]
fn test_require_not_reserved_rejects_admin_without_explicit_admin() {
    let (env, admin, treasury, contract_addr) = setup_initialized();
    let contract_id = contract_addr.clone();
    let err =
        call_require_not_reserved(&env, &contract_id, &admin, None, None, Some(contract_addr));
    assert_eq!(err, Err(QuickLendXError::InvalidCurrency));
}

// ============================================================================
// Initialization rejection tests (existing)
// ============================================================================

#[test]
fn test_init_rejects_admin_equals_treasury() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let params = base_params(admin.clone(), admin.clone(), Vec::new(&env));

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAddress)));
}

#[test]
fn test_init_rejects_admin_as_contract_address() {
    let (env, client) = setup();
    let admin = client.address.clone();
    let treasury = Address::generate(&env);
    let params = base_params(admin, treasury, Vec::new(&env));

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAddress)));
}

#[test]
fn test_init_rejects_treasury_as_contract_address() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = client.address.clone();
    let params = base_params(admin, treasury, Vec::new(&env));

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidAddress)));
}

#[test]
fn test_init_rejects_duplicate_currencies() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let currency = Address::generate(&env);
    let currencies = Vec::from_array(&env, [currency.clone(), currency]);
    let params = base_params(admin, treasury, currencies);

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidCurrency)));
}

#[test]
fn test_init_rejects_currency_conflicts() {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let contract_currency = client.address.clone();
    let currencies = Vec::from_array(&env, [admin.clone(), treasury.clone(), contract_currency]);
    let params = base_params(admin, treasury, currencies);

    let result = client.try_initialize(&params);
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidCurrency)));
}
