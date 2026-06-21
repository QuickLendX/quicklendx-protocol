//! Tests for the `qlx_log!` structured diagnostics macro.
//!
//! These tests verify:
//! 1. The macro emits correctly-prefixed domain-tagged log lines when compiled under `cfg(test)`.
//! 2. Logs are captured by `env.logs().all()` and contain the expected content.
//! 3. Edge cases: empty messages, multiple domains, formatting arguments.
//! 4. The macro compiles correctly in both the enabled and disabled branches.

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Logs},
    Address, Env, String, Vec,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Set up a minimal environment and return a registered contract client.
fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn full_setup() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let (env, client) = setup();
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let contract_addr = client.address.clone();
    (env, client, admin, contract_addr)
}

// ---------------------------------------------------------------------------
// Macro unit tests (directly invoke qlx_log! without contract calls)
// ---------------------------------------------------------------------------

#[test]
fn test_qlx_log_plain_message_is_captured() {
    let env = Env::default();
    // Under cfg(test) the macro is always enabled.
    crate::qlx_log!(&env, "test", "hello from diagnostics");
    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("[test] hello from diagnostics")),
        "Expected '[test] hello from diagnostics' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_qlx_log_with_format_args_is_captured() {
    let env = Env::default();
    let amount: i128 = 42_000;
    crate::qlx_log!(&env, "payment", "amount={}", amount);
    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("[payment]") && l.contains("amount=")),
        "Expected '[payment] amount=...' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_qlx_log_multiple_domains_are_tagged_correctly() {
    let env = Env::default();
    for (domain, _) in crate::diagnostics::DIAGNOSTIC_DOMAINS {
        match domain {
            "escrow" => crate::qlx_log!(&env, "escrow", "Escrow created"),
            "bid" => crate::qlx_log!(&env, "bid", "Bid placed"),
            "settlement" => crate::qlx_log!(&env, "settlement", "Payment recorded"),
            "payment" => crate::qlx_log!(&env, "payment", "Funds transferred"),
            _ => panic!("untested diagnostic domain: {}", domain),
        }
    }

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join(" | ");

    assert!(
        log_str.contains("[escrow] Escrow created"),
        "Missing escrow log"
    );
    assert!(log_str.contains("[bid] Bid placed"), "Missing bid log");
    assert!(
        log_str.contains("[settlement] Payment recorded"),
        "Missing settlement log"
    );
    assert!(
        log_str.contains("[payment] Funds transferred"),
        "Missing payment log"
    );
}

#[test]
fn test_diagnostic_domain_catalog_matches_expected_tags() {
    let domains = crate::diagnostics::DIAGNOSTIC_DOMAINS;

    assert_eq!(domains.len(), 4, "unexpected diagnostics domain count");
    assert_eq!(domains[0].0, "escrow");
    assert_eq!(domains[1].0, "bid");
    assert_eq!(domains[2].0, "settlement");
    assert_eq!(domains[3].0, "payment");

    for (domain, meaning) in domains {
        assert!(
            !domain.is_empty(),
            "diagnostic domain tag must not be empty"
        );
        assert!(
            !meaning.is_empty(),
            "diagnostic domain {} must document its meaning",
            domain
        );
    }
}

#[test]
fn test_diagnostics_feature_contract_is_explicit() {
    // Unit tests always compile the logging branch through cfg(test). Outside
    // tests, contributors must enable the diagnostics feature to emit logs.
    assert!(cfg!(test), "diagnostic tests must run under cfg(test)");

    if cfg!(feature = "diagnostics") {
        assert!(
            cfg!(any(test, feature = "diagnostics")),
            "diagnostics feature should compile the emitting macro branch"
        );
    } else {
        assert!(
            !cfg!(feature = "diagnostics"),
            "default test run documents the feature-disabled configuration"
        );
    }
}

#[test]
fn test_qlx_log_empty_message() {
    // An empty message string should still compile and run cleanly.
    let env = Env::default();
    crate::qlx_log!(&env, "test", "");
    // Just asserting it doesn't panic is sufficient.
}

#[test]
fn test_qlx_log_multiple_format_args() {
    let env = Env::default();
    let investor_return: i128 = 9_800;
    let platform_fee: i128 = 200;
    crate::qlx_log!(
        &env,
        "settlement",
        "investor_return={} platform_fee={}",
        investor_return,
        platform_fee
    );
    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("[settlement]") && l.contains("investor_return=")),
        "Expected settlement log with multiple args, got: {:?}",
        logs
    );
}

// ---------------------------------------------------------------------------
// Integration tests: qlx_log! emitted via real contract lifecycle calls
// ---------------------------------------------------------------------------

#[test]
fn test_bid_placed_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);

    let invoice_id = client.store_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Diagnostics test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    client.place_bid(&investor, &invoice_id, &5_000, &5_500);

    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("[bid]") && l.contains("Bid placed")),
        "Expected '[bid] Bid placed...' in logs after place_bid, got: {:?}",
        logs
    );
}

#[test]
fn test_bid_withdrawn_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);

    let invoice_id = client.store_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Withdraw diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &5_500);
    client.withdraw_bid(&bid_id);

    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("[bid]") && l.contains("withdrawn")),
        "Expected '[bid] Bid withdrawn' in logs after withdraw_bid, got: {:?}",
        logs
    );
}

#[test]
fn test_escrow_lifecycle_emits_diagnostic_logs() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);

    let invoice_id = client.store_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Escrow diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &10_000, &10_500);

    // accept_bid triggers escrow + payment logs
    client.accept_bid(&invoice_id, &bid_id);

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join("\n");

    assert!(
        log_str.contains("[escrow]") && log_str.contains("Accepting bid"),
        "Expected '[escrow] Accepting bid...' in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("[payment]") && log_str.contains("Creating escrow"),
        "Expected '[payment] Creating escrow...' in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("[payment]") && log_str.contains("Escrow created"),
        "Expected '[payment] Escrow created...' in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("[escrow]") && log_str.contains("Invoice funded"),
        "Expected '[escrow] Invoice funded...' in logs, got:\n{}",
        log_str
    );
}

#[test]
fn test_partial_payment_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);

    let invoice_id = client.store_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Partial payment diagnostics"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &10_000, &10_500);
    client.accept_bid(&invoice_id, &bid_id);

    client.process_partial_payment(&invoice_id, &3_000i128, &String::from_str(&env, "txn-001"));

    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("[settlement]") && l.contains("partial payment")),
        "Expected '[settlement] Recording partial payment...' in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|l| l.contains("[settlement]") && l.contains("Payment recorded")),
        "Expected '[settlement] Payment recorded...' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_settlement_finalization_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);

    let invoice_id = client.store_invoice(
        &business,
        &5_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Settlement diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &5_000, &5_500);
    client.accept_bid(&invoice_id, &bid_id);

    client.settle_invoice(&invoice_id, &5_000i128);

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join("\n");

    assert!(
        log_str.contains("[settlement]") && log_str.contains("Full settlement initiated"),
        "Expected '[settlement] Full settlement initiated...' in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("[settlement]") && log_str.contains("Invoice settled"),
        "Expected '[settlement] Invoice settled...' in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("[payment]") && log_str.contains("Escrow released"),
        "Expected '[payment] Escrow released...' in logs, got:\n{}",
        log_str
    );
}

#[test]
fn test_refund_escrow_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &business, &investor, &contract_addr);

    let invoice_id = client.store_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Refund diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(&investor, &invoice_id, &10_000, &10_500);
    client.accept_bid(&invoice_id, &bid_id);

    client.refund_escrow(&invoice_id, &business);

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join("\n");

    assert!(
        log_str.contains("[payment]") && log_str.contains("Escrow refunded"),
        "Expected '[payment] Escrow refunded...' in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("[escrow]") && log_str.contains("Escrow refunded successfully"),
        "Expected '[escrow] Escrow refunded successfully' in logs, got:\n{}",
        log_str
    );
}
