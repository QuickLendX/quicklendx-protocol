use super::*;
use soroban_sdk::{
    testutils::Address as _, Address, Env, String,
};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn submit_business_kyc(env: &Env, client: &QuickLendXContractClient, business: &Address) {
    client.submit_kyc_application(business, &String::from_str(env, "KYC"));
}

#[test]
fn test_admin_bootstrap_blocks_privileged_actions() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let business = Address::generate(&env);

    assert!(client.get_admin().is_none());

    let verify_result = client.try_verify_business(&admin, &business);
    assert!(verify_result.is_err(), "Uninitialized admin must block verification");

    let fee_result = client.try_set_platform_fee(&250);
    assert!(fee_result.is_err(), "Uninitialized admin must block fee config");

    client.set_admin(&admin);
    assert_eq!(client.get_admin(), Some(admin));
}

#[test]
fn test_non_admin_cannot_verify_business() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let business = Address::generate(&env);

    client.set_admin(&admin);
    submit_business_kyc(&env, &client, &business);

    let result = client.try_verify_business(&non_admin, &business);
    assert!(result.is_err(), "Non-admin verification must fail");

    let status = client.get_business_verification_status(&business);
    assert!(status.is_some());
    assert_eq!(status.unwrap().status, BusinessVerificationStatus::Pending);
}

#[test]
fn test_admin_rotation_transfers_privileges() {
    let (env, client) = setup();
    env.mock_all_auths();
    let admin_one = Address::generate(&env);
    let admin_two = Address::generate(&env);
    let business = Address::generate(&env);

    client.set_admin(&admin_one);
    client.set_admin(&admin_two);
    assert_eq!(client.get_admin(), Some(admin_two.clone()));

    submit_business_kyc(&env, &client, &business);

    let old_admin_result = client.try_verify_business(&admin_one, &business);
    assert!(old_admin_result.is_err(), "Old admin must lose privileges");

    let new_admin_result = client.try_verify_business(&admin_two, &business);
    assert!(new_admin_result.is_ok(), "New admin must retain privileges");

    let status = client.get_business_verification_status(&business).unwrap();
    assert_eq!(status.status, BusinessVerificationStatus::Verified);
    assert_eq!(status.verified_by, Some(admin_two));
}
