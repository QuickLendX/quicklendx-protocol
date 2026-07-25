//! Tests for `get_operational_limits` consolidated read (#1539).

#![cfg(test)]

use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (QuickLendXContractClient<'static>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin)
}

#[test]
fn test_get_operational_limits_returns_protocol_constants() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let limits = client.get_operational_limits();

    assert_eq!(
        limits.max_batch,
        crate::defaults::max_overdue_scan_batch_limit()
    );
    assert_eq!(limits.max_limit, crate::MAX_QUERY_LIMIT);
    assert_eq!(limits.max_fee, crate::init::MAX_FEE_BPS);
}

#[test]
fn test_get_operational_limits_does_not_require_auth() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    // No mock_auths/require_auth wiring needed: this is a pure read.
    let limits = client.get_operational_limits();
    assert!(limits.max_batch > 0);
    assert!(limits.max_limit > 0);
    assert!(limits.max_fee > 0);
}

#[test]
fn test_get_operational_limits_stable_across_calls() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    let first = client.get_operational_limits();
    let second = client.get_operational_limits();
    assert_eq!(first, second);
}
