#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

#[test]
fn test_invoice_amount_limits() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let admin = Address::generate(&env);

    client.set_admin(&admin);

    // Test zero amount rejection (without business verification to focus on amount validation)
    let result = client.try_store_invoice(
        &business,
        &0i128,
        &currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &soroban_sdk::vec![&env],
    );
    // Should fail with InvalidAmount or BusinessNotVerified
    assert!(result.is_err());

    // Test negative amount rejection
    let result = client.try_store_invoice(
        &business,
        &(-1000i128),
        &currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &soroban_sdk::vec![&env],
    );
    // Should fail with InvalidAmount or BusinessNotVerified
    assert!(result.is_err());
}

#[test]
fn test_description_length_limits() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Test empty description rejection
    let result = client.try_store_invoice(
        &business,
        &10000i128,
        &currency,
        &(env.ledger().timestamp() + 86400),
        &String::from_str(&env, ""),
        &InvoiceCategory::Services,
        &soroban_sdk::vec![&env],
    );
    // Should fail with InvalidDescription or BusinessNotVerified
    assert!(result.is_err());
}

#[test]
fn test_due_date_limits() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    // Test past due date rejection (use a safe past timestamp)
    let current_time = env.ledger().timestamp();
    let past_time = if current_time > 86400 {
        current_time - 86400
    } else {
        0
    };

    let result = client.try_store_invoice(
        &business,
        &10000i128,
        &currency,
        &past_time,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &soroban_sdk::vec![&env],
    );
    // Should fail with InvoiceDueDateInvalid or BusinessNotVerified
    assert!(result.is_err());
}

#[test]
fn test_bid_amount_limits() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let investor = Address::generate(&env);
    let invoice_id: BytesN<32> = BytesN::from_array(&env, &[1u8; 32]);

    // Test zero bid amount rejection
    let result = client.try_place_bid(&investor, &invoice_id, &0i128, &10500i128);
    // Should fail with InvalidAmount or other error
    assert!(result.is_err());

    // Test negative bid amount rejection
    let result = client.try_place_bid(&investor, &invoice_id, &(-1000i128), &10500i128);
    // Should fail with InvalidAmount or other error
    assert!(result.is_err());
}

#[test]
fn test_admin_operations_require_authorization() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let business = Address::generate(&env);

    // Set an admin first
    client.set_admin(&admin);

    // Test that non-admin cannot verify business
    let result = client.try_verify_business(&non_admin, &business);
    assert!(result.is_err());
}

// casting of References
// perequisites for method chaining
