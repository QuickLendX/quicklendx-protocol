#![cfg(test)]

use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    (client, admin)
}

#[test]
fn test_initialize_admin_enables_legacy_admin_paths() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);

    client.initialize_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);

    assert_eq!(client.get_current_admin(), Some(admin.clone()));
    assert_eq!(client.get_admin(), Some(admin.clone()));
}

#[test]
fn test_legacy_set_admin_keeps_pause_and_emergency_paths_consistent() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.set_admin(&admin);
    client.pause(&admin);
    client.initiate_emergency_withdraw(&admin, &token, &500i128, &target);

    assert!(client.is_paused());
    assert!(client.get_pending_emergency_withdraw().is_some());
    assert_eq!(client.get_current_admin(), Some(admin.clone()));
}

#[test]
fn test_transfer_admin_reassigns_pause_exempt_authority() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);
    let currency = Address::generate(&env);

    client.initialize_admin(&admin);
    client.pause(&admin);
    client.transfer_admin(&new_admin);

    let old_admin_err = client
        .try_add_currency(&admin, &currency)
        .err()
        .expect("old admin should be rejected")
        .expect("contract error");
    assert_eq!(old_admin_err, QuickLendXError::NotAdmin);

    client.add_currency(&new_admin, &currency);
    assert!(client.is_allowed_currency(&currency));
}

#[test]
fn test_require_admin_auth_rejects_non_admin_on_pause_exempt_paths() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let non_admin = Address::generate(&env);
    let currency = Address::generate(&env);
    let token = Address::generate(&env);
    let target = Address::generate(&env);

    client.initialize_admin(&admin);
    client.pause(&admin);

    let unpause_err = client
        .try_unpause(&non_admin)
        .err()
        .expect("non-admin unpause should fail")
        .expect("contract error");
    assert_eq!(unpause_err, QuickLendXError::NotAdmin);

    let add_currency_err = client
        .try_add_currency(&non_admin, &currency)
        .err()
        .expect("non-admin currency update should fail")
        .expect("contract error");
    assert_eq!(add_currency_err, QuickLendXError::NotAdmin);

    let emergency_err = client
        .try_initiate_emergency_withdraw(&non_admin, &token, &100i128, &target)
        .err()
        .expect("non-admin emergency flow should fail")
        .expect("contract error");
    assert_eq!(emergency_err, QuickLendXError::NotAdmin);
}

#[test]
fn test_pause_does_not_reopen_invoice_verification_for_admin() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.initialize_admin(&admin);
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);

    let err = client
        .try_verify_invoice(&invoice_id)
        .err()
        .expect("invoice verification must stay blocked while paused")
        .expect("contract error");
    assert_eq!(err, QuickLendXError::OperationNotAllowed);
}
