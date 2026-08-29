/// Tests for the `regulatory` module.
///
/// Verifies that `require_regulatory_ok` is a stable no-op seam: it always
/// returns `Ok(())` and never blocks callers today.
use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    (env, client, admin)
}

/// `require_regulatory_ok` must always return `Ok(())` for any caller.
#[test]
fn test_require_regulatory_ok_is_noop() {
    let env = Env::default();
    for _ in 0..20 {
        let addr = Address::generate(&env);
        assert!(crate::regulatory::require_regulatory_ok(&env, &addr).is_ok());
    }
}

/// `store_invoice` must succeed even though the regulatory gate is active.
/// This locks in the no-op contract: the hook is called but does not block.
#[test]
fn test_store_invoice_regulatory_gate_is_noop() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    assert!(
        result.is_ok(),
        "store_invoice must not be blocked by the no-op regulatory gate"
    );
}

/// `place_bid` must succeed even though the regulatory gate is active.
/// This locks in the no-op contract: the hook is called but does not block.
#[test]
fn test_place_bid_regulatory_gate_is_noop() {
    let (env, client, admin) = setup();
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;
    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None,
    );
    assert!(
        result.is_ok(),
        "store_invoice must not be blocked by the no-op regulatory gate"
    );
    let invoice_id = result.unwrap().unwrap();

    client.verify_invoice(&invoice_id);

    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "KYC data"));
    client.verify_investor(&investor, &10_000i128);

    let bid_result = client.try_place_bid(
        &investor,
        &invoice_id,
        &1_000i128,
        &1_100i128,
        &soroban_sdk::BytesN::from_array(&env, &[0u8; 32]),
    );
    assert!(
        bid_result.is_ok(),
        "place_bid must not be blocked by the no-op regulatory gate"
    );
    let _bid_id = bid_result.unwrap().unwrap();
}
