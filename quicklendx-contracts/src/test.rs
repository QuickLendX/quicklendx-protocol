#![cfg(test)]
use super::*;
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn allows_invoice_creation_when_unfrozen() {
    let env = Env::default();
    env.mock_all_signatures();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let issuer = Address::generate(&env);
    let invoice_id = 101u64;

    let result = client.try_create_invoice(&issuer, &invoice_id);
    assert!(result.is_ok());
}

#[test]
fn blocks_invoice_creation_when_frozen() {
    let env = Env::default();
    env.mock_all_signatures();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let invoice_id = 102u64;

    client.freeze(&admin, &issuer);

    let result = client.try_create_invoice(&issuer, &invoice_id);
    assert_eq!(result, Err(Ok(QuickLendXError::AccountIsFrozen)));
}

#[test]
fn allows_invoice_creation_after_unfreeze() {
    let env = Env::default();
    env.mock_all_signatures();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    let invoice_id = 103u64;

    client.freeze(&admin, &issuer);
    let failed_result = client.try_create_invoice(&issuer, &invoice_id);
    assert_eq!(failed_result, Err(Ok(QuickLendXError::AccountIsFrozen)));

    client.unfreeze(&admin, &issuer);
    let success_result = client.try_create_invoice(&issuer, &invoice_id);
    assert!(success_result.is_ok());
}
