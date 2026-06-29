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
use crate::storage::InvoiceStorage;
use crate::types::{Investment, InvestmentStatus, Invoice, InvoiceStatus, InvoiceCategory, Dispute, DisputeStatus};
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

/// Build an Invoice record.
fn make_invoice(env: &Env, invoice_id: &BytesN<32>) -> Invoice {
    let business = Address::generate(env);
    Invoice {
        id: invoice_id.clone(),
        business: business.clone(),
        amount: 10_000,
        currency: Address::generate(env),
        due_date: 0,
        status: InvoiceStatus::Pending,
        created_at: 0,
        description: String::from_str(env, "desc"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(env),
        category: InvoiceCategory::Services,
        tags: Vec::new(env),
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: Vec::new(env),
        dispute_status: DisputeStatus::None,
        dispute: Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            evidence_hash: None,
            resolution: String::from_str(env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        },
        total_paid: 0,
        payment_history: Vec::new(env),
    }
}

#[test]
fn test_fresh_contract_all_pass() {
    let (_env, client, _id, admin) = setup();

    let report = client.invariant_self_check(&admin);

    // Seven composed checks, all green on an empty protocol.
    assert_eq!(report.checks.len(), 7);
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
        let invoice = make_invoice(&env, &investment.invoice_id);
        InvoiceStorage::store_invoice(&env, &invoice);
        InvestmentStorage::store_investment(&env, &investment);
        run_invariant_checks(&env)
    });

    assert!(report.all_passed);
    assert!(passed_for(&env, &report, "no_orphan_investments"));
    assert!(passed_for(&env, &report, "solvency"));
    assert!(passed_for(&env, &report, "sum_investments_le_sum_invoices"));
    assert!(passed_for(&env, &report, "escrow_uniqueness"));
    assert!(passed_for(&env, &report, "settlement_accounting_identity"));
}

#[test]
fn test_simulated_tampering_is_detected() {
    let (env, _client, contract_id, _admin) = setup();

    let report = env.as_contract(&contract_id, || {
        let investment = make_active_investment(&env);
        let invoice = make_invoice(&env, &investment.invoice_id);
        InvoiceStorage::store_invoice(&env, &invoice);
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
        let invoice = make_invoice(&env, &investment.invoice_id);
        InvoiceStorage::store_invoice(&env, &invoice);
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

#[test]
fn test_sum_investments_le_sum_invoices_violation() {
    let (env, _client, contract_id, _admin) = setup();

    let report = env.as_contract(&contract_id, || {
        let investment = make_active_investment(&env);
        let mut invoice = make_invoice(&env, &investment.invoice_id);
        // Make invoice amount less than investment amount to trigger violation
        invoice.amount = investment.amount - 100;
        InvoiceStorage::store_invoice(&env, &invoice);
        InvestmentStorage::store_investment(&env, &investment);
        run_invariant_checks(&env)
    });

    assert!(!passed_for(
        &env,
        &report,
        "sum_investments_le_sum_invoices"
    ));
    assert!(!report.all_passed);
}

#[test]
fn test_escrow_uniqueness_violation() {
    let (env, _client, contract_id, _admin) = setup();

    let report = env.as_contract(&contract_id, || {
        let investment = make_active_investment(&env);
        let invoice = make_invoice(&env, &investment.invoice_id);
        InvoiceStorage::store_invoice(&env, &invoice);
        InvestmentStorage::store_investment(&env, &investment);

        // Tamper: store a corrupted escrow mapping where the escrow points back to a different invoice ID
        let escrow_id = BytesN::from_array(&env, &[9u8; 32]);
        let corrupted_escrow = crate::payments::Escrow {
            escrow_id: escrow_id.clone(),
            invoice_id: BytesN::from_array(&env, &[99u8; 32]), // Mismatched invoice ID
            investor: Address::generate(&env),
            business: Address::generate(&env),
            amount: 500,
            currency: Address::generate(&env),
            created_at: 0,
            status: crate::payments::EscrowStatus::Held,
        };
        env.storage()
            .persistent()
            .set(&corrupted_escrow.escrow_id, &corrupted_escrow);

        let invoice_key = (soroban_sdk::symbol_short!("escrow"), &invoice.id);
        env.storage().persistent().set(&invoice_key, &escrow_id);

        run_invariant_checks(&env)
    });

    assert!(!passed_for(&env, &report, "escrow_uniqueness"));
    assert!(!report.all_passed);
}

#[test]
fn test_settlement_accounting_identity_violation() {
    let (env, _client, contract_id, _admin) = setup();

    let report = env.as_contract(&contract_id, || {
        let investment = make_active_investment(&env);
        let mut invoice = make_invoice(&env, &investment.invoice_id);
        invoice.status = InvoiceStatus::Paid;
        invoice.total_paid = investment.amount; // healthy total_paid matches investment.amount
        InvoiceStorage::store_invoice(&env, &invoice);
        InvestmentStorage::store_investment(&env, &investment);

        // Verify it passes first
        let healthy_report = run_invariant_checks(&env);
        assert!(passed_for(
            &env,
            &healthy_report,
            "settlement_accounting_identity"
        ));

        // Tamper: corrupt total_paid to a negative number to violate accounting identity
        let mut tampered_invoice = invoice.clone();
        tampered_invoice.total_paid = -100;
        InvoiceStorage::store_invoice(&env, &tampered_invoice);

        run_invariant_checks(&env)
    });

    assert!(!passed_for(&env, &report, "settlement_accounting_identity"));
    assert!(!report.all_passed);
}
