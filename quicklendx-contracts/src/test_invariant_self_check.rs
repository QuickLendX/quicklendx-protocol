//! Tests for the admin-callable `invariant_self_check` heartbeat.
//!
//! Covers the required edge cases: a fresh contract, a populated/healthy state
//! (proxy for a post-lifecycle ledger), and simulated tampering. Also asserts
//! the security property that the check is admin-gated and never mutates state.

#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, String, Vec};

use crate::invariants::{run_invariant_checks, InvariantReport};
use crate::investment::InvestmentStorage;
use crate::types::{Investment, InvestmentStatus};
use crate::{QuickLendXContract, QuickLendXContractClient};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.initialize_admin(&admin);
    (env, client, contract_id, admin)
}

/// Look up a single check's `passed` flag by its stable name.
fn passed_for(env: &Env, report: &InvariantReport, name: &str) -> bool {
    let target = String::from_str(env, name);
    for check in report.checks.iter() {
        if check.check_name == target {
            return check.passed;
        }
    }
    panic!("check not found in report");
}

/// Build an Active investment record for direct-storage scenarios.
fn make_active_investment(env: &Env) -> Investment {
    Investment {
        investment_id: InvestmentStorage::generate_unique_investment_id(env),
        invoice_id: BytesN::from_array(env, &[7u8; 32]),
        investor: Address::generate(env),
        amount: 1_000,
        funded_at: 0,
        status: InvestmentStatus::Active,
        insurance: Vec::new(env),
    }
}

#[test]
fn test_fresh_contract_all_pass() {
    let (_env, client, _id, admin) = setup();

    let report = client.invariant_self_check(&admin);

    // Four composed checks, all green on an empty protocol.
    assert_eq!(report.checks.len(), 4);
    assert!(report.all_passed);
}

#[test]
fn test_non_admin_is_rejected() {
    let (_env, client, _id, _admin) = setup();
    let stranger = Address::generate(&_env);

    // Auth is mocked, so require_auth passes; the admin-equality gate must not.
    let result = client.try_invariant_self_check(&stranger);
    assert!(result.is_err());
}

#[test]
fn test_populated_healthy_state_passes() {
    // Proxy for a post-lifecycle ledger: a real Active investment present.
    let (env, _client, contract_id, _admin) = setup();

    let report = env.as_contract(&contract_id, || {
        let investment = make_active_investment(&env);
        InvestmentStorage::store_investment(&env, &investment);
        run_invariant_checks(&env)
    });

    assert!(report.all_passed);
    assert!(passed_for(&env, &report, "no_orphan_investments"));
    assert!(passed_for(&env, &report, "solvency"));
}

#[test]
fn test_simulated_tampering_is_detected() {
    let (env, _client, contract_id, _admin) = setup();

    let report = env.as_contract(&contract_id, || {
        let investment = make_active_investment(&env);
        // Store as Active so it lands in the active-investment index.
        InvestmentStorage::store_investment(&env, &investment);

        // Tamper: persist a terminal status directly, bypassing the normal
        // update path that would have de-indexed it. This fabricates an orphan.
        let mut tampered = investment.clone();
        tampered.status = InvestmentStatus::Defaulted;
        env.storage()
            .persistent()
            .set(&tampered.investment_id, &tampered);

        run_invariant_checks(&env)
    });

    assert!(!passed_for(&env, &report, "no_orphan_investments"));
    assert!(!report.all_passed);
}

#[test]
fn test_self_check_never_modifies_state() {
    let (env, _client, contract_id, _admin) = setup();

    env.as_contract(&contract_id, || {
        let investment = make_active_investment(&env);
        InvestmentStorage::store_investment(&env, &investment);

        let active_before = InvestmentStorage::get_active_investment_ids(&env).len();
        let first = run_invariant_checks(&env);
        let active_after = InvestmentStorage::get_active_investment_ids(&env).len();

        // Read-only: the active index is untouched by running the check.
        assert_eq!(active_before, active_after);

        // Deterministic on unchanged state and ledger time.
        let second = run_invariant_checks(&env);
        assert_eq!(first, second);
    });
}
