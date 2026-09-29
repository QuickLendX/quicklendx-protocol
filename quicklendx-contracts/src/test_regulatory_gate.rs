//! Tests confirming `verification::require_regulatory_ok` is truly a no-op
//! by default (issue #1920).
//!
//! `require_regulatory_ok` is a placeholder hook reserved for future
//! jurisdiction/sanctions-list checks. No such checks are implemented yet,
//! so every call must unconditionally succeed regardless of the address
//! passed in or any KYC/verification state already recorded for it. These
//! run on every CI matrix entry (plain `#[cfg(test)]`, no feature gate).

#![cfg(test)]

use crate::verification::require_regulatory_ok;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.set_admin(&admin);
    (client, admin)
}

#[test]
fn returns_ok_for_an_address_with_no_recorded_state() {
    let env = Env::default();
    let address = Address::generate(&env);

    assert!(
        require_regulatory_ok(&env, &address).is_ok(),
        "the gate must succeed by default for an address with no history at all"
    );
}

#[test]
fn returns_ok_without_requiring_authorization() {
    // Deliberately skip `env.mock_all_auths()`. A true no-op never calls
    // `require_auth()`; if it ever started doing so, this would panic
    // instead of returning `Ok`.
    let env = Env::default();
    let address = Address::generate(&env);

    require_regulatory_ok(&env, &address).unwrap();
}

#[test]
fn returns_ok_for_a_business_with_pending_kyc() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "pending kyc"));

    assert!(
        require_regulatory_ok(&env, &business).is_ok(),
        "gate must stay a no-op even for a business stuck in KYC-pending state"
    );
}

#[test]
fn returns_ok_for_a_rejected_business() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "will be rejected"));
    client.reject_business(&admin, &business, &String::from_str(&env, "not eligible"));

    assert!(
        require_regulatory_ok(&env, &business).is_ok(),
        "gate must stay a no-op even for a rejected business"
    );
}

#[test]
fn returns_ok_for_a_verified_business() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "verified kyc"));
    client.verify_business(&admin, &business);

    assert!(require_regulatory_ok(&env, &business).is_ok());
}

#[test]
fn returns_ok_for_many_distinct_addresses() {
    let env = Env::default();
    for _ in 0..100 {
        let address = Address::generate(&env);
        assert!(require_regulatory_ok(&env, &address).is_ok());
    }
}

#[test]
fn is_idempotent_across_repeated_calls() {
    let env = Env::default();
    let address = Address::generate(&env);

    for _ in 0..10 {
        assert!(require_regulatory_ok(&env, &address).is_ok());
    }
}
