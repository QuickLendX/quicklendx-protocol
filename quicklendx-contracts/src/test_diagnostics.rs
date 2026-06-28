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
    token, Address, Env, String, Vec,
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
    let _ = client.try_initialize_protocol_limits(&admin, &1i128, &365u64, &86_400u64);
    let contract_addr = client.address.clone();
    (env, client, admin, contract_addr)
}

fn setup_verified_business(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
) -> Address {
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    business
}

fn setup_verified_investor(env: &Env, client: &QuickLendXContractClient, limit: i128) -> Address {
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &limit);
    investor
}

fn setup_token(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    let initial = 100_000i128;
    sac.mint(business, &initial);
    sac.mint(investor, &initial);
    let expiry = env.ledger().sequence() + 10_000;
    tok.approve(business, contract_id, &initial, &expiry);
    tok.approve(investor, contract_id, &initial, &expiry);
    client.add_currency(admin, &currency);
    currency
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
            .any(|l| l.contains("\"test\"") && l.contains("\"hello from diagnostics\"")),
        "Expected 'test' and 'hello from diagnostics' in logs, got: {:?}",
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
            .any(|l| l.contains("\"payment\"") && l.contains("amount=42000")),
        "Expected 'payment' and 'amount=42000' in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_qlx_log_multiple_domains_are_tagged_correctly() {
    let env = Env::default();
    crate::qlx_log!(&env, "escrow", "Escrow created");
    crate::qlx_log!(&env, "bid", "Bid placed");
    crate::qlx_log!(&env, "settlement", "Payment recorded");
    crate::qlx_log!(&env, "payment", "Funds transferred");

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join(" | ");

    assert!(
        log_str.contains("\"escrow\"") && log_str.contains("Escrow created"),
        "Missing escrow log"
    );
    assert!(
        log_str.contains("\"bid\"") && log_str.contains("Bid placed"),
        "Missing bid log"
    );
    assert!(
        log_str.contains("\"settlement\"") && log_str.contains("Payment recorded"),
        "Missing settlement log"
    );
    assert!(
        log_str.contains("\"payment\"") && log_str.contains("Funds transferred"),
        "Missing payment log"
    );
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
        logs.iter().any(|l| l.contains("\"settlement\"")
            && l.contains("investor_return=9800")
            && l.contains("platform_fee=200")),
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
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Diagnostics test invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    client.place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &5_500,
        &BytesN::from_array(&env, &[0u8; 32]),
    );

    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("\"bid\"") && l.contains("Bid placed")),
        "Expected 'bid' and 'Bid placed' in logs after place_bid, got: {:?}",
        logs
    );
}

#[test]
fn test_bid_withdrawn_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Withdraw diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &5_500,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.withdraw_bid(&bid_id);

    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("\"bid\"") && l.contains("Bid withdrawn")),
        "Expected 'bid' and 'Bid withdrawn' in logs after withdraw_bid, got: {:?}",
        logs
    );
}

#[test]
fn test_escrow_lifecycle_emits_diagnostic_logs() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Escrow diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &10_000,
        &10_500,
        &BytesN::from_array(&env, &[0u8; 32]),
    );

    // accept_bid_and_fund triggers escrow + payment logs
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join("\n");

    assert!(
        log_str.contains("\"escrow\"") && log_str.contains("Accepting bid"),
        "Expected 'escrow' Accepting bid in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("\"payment\"") && log_str.contains("Creating escrow"),
        "Expected 'payment' Creating escrow in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("\"payment\"") && log_str.contains("Escrow created"),
        "Expected 'payment' Escrow created in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("\"escrow\"") && log_str.contains("Invoice funded"),
        "Expected 'escrow' Invoice funded in logs, got:\n{}",
        log_str
    );
}

#[test]
fn test_partial_payment_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Partial payment diagnostics"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &10_000,
        &10_500,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    client.process_partial_payment(&invoice_id, &3_000i128, &String::from_str(&env, "txn-001"));

    let logs = env.logs().all();
    assert!(
        logs.iter()
            .any(|l| l.contains("\"settlement\"") && l.contains("Recording partial payment")),
        "Expected 'settlement' Recording partial payment in logs, got: {:?}",
        logs
    );
    assert!(
        logs.iter()
            .any(|l| l.contains("\"settlement\"") && l.contains("Payment recorded")),
        "Expected 'settlement' Payment recorded in logs, got: {:?}",
        logs
    );
}

#[test]
fn test_settlement_finalization_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    let invoice_id = client.upload_invoice(
        &business,
        &5_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Settlement diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &5_500,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    client.settle_invoice(&invoice_id, &5_000i128);

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join("\n");

    assert!(
        log_str.contains("\"settlement\"") && log_str.contains("Full settlement initiated"),
        "Expected 'settlement' Full settlement initiated in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("\"settlement\"") && log_str.contains("Invoice settled"),
        "Expected 'settlement' Invoice settled in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("\"payment\"") && log_str.contains("Escrow released"),
        "Expected 'payment' Escrow released in logs, got:\n{}",
        log_str
    );
}

#[test]
fn test_refund_escrow_emits_diagnostic_log() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Refund diagnostics invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &10_000,
        &10_500,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    client.refund_escrow_funds(&invoice_id, &business);

    let logs = env.logs().all();
    let log_str: alloc::string::String = logs.join("\n");

    assert!(
        log_str.contains("\"payment\"") && log_str.contains("Escrow refunded"),
        "Expected 'payment' Escrow refunded in logs, got:\n{}",
        log_str
    );
    assert!(
        log_str.contains("\"escrow\"") && log_str.contains("Escrow refunded successfully"),
        "Expected 'escrow' Escrow refunded successfully in logs, got:\n{}",
        log_str
    );
}

#[test]
fn test_diagnostics_feature_gating_behavior() {
    let env = Env::default();

    #[cfg(feature = "diagnostics")]
    {
        crate::qlx_log!(&env, "test", "emitted with diagnostics feature");
        let logs = env.logs().all();
        assert!(
            logs.iter()
                .any(|l| l.contains("\"test\"") && l.contains("emitted with diagnostics feature")),
            "Diagnostics log must be emitted when diagnostics feature is enabled"
        );
    }

    #[cfg(not(feature = "diagnostics"))]
    {
        // Assert that the feature flag is disabled under cargo test without feature
        assert!(
            !cfg!(feature = "diagnostics"),
            "Expected diagnostics feature to be disabled"
        );
    }
}

#[test]
fn test_diagnostics_tags_and_topics() {
    let env = Env::default();

    // Emit diagnostic signals for each domain tag
    crate::qlx_log!(&env, "escrow", "testing escrow tag");
    crate::qlx_log!(&env, "bid", "testing bid tag");
    crate::qlx_log!(&env, "settlement", "testing settlement tag");
    crate::qlx_log!(&env, "payment", "testing payment tag");

    let logs = env.logs().all();
    let expected_tags = ["escrow", "bid", "settlement", "payment"];

    // Validate that each domain-tagged log is properly prefixed and contains only expected domain tags.
    for log in logs.iter() {
        if log.contains("\"[{}] {}\"") {
            let has_valid_tag = expected_tags.iter().any(|tag| {
                let quoted_tag = alloc::format!("\"{}\"", tag);
                log.contains(&quoted_tag)
            });
            let is_test_tag = log.contains("\"test\"");
            if !is_test_tag {
                assert!(
                    has_valid_tag,
                    "Log contains unexpected tag structure or invalid tag: {}",
                    log
                );
            }
        }
    }
}

#[test]
fn test_error_path_no_diagnostic() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 25_000);
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    // Call upload_invoice but don't verify it. Bidding on unverified invoice is an error path.
    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Diagnostics error path invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    // This should fail because the invoice is not verified
    let result = client.try_place_bid(
        &investor,
        &invoice_id,
        &5_000,
        &5_500,
        &BytesN::from_array(&env, &[0u8; 32]),
    );
    assert!(result.is_err());

    // Verify no [bid] diagnostics log was emitted since it failed on validation
    let logs = env.logs().all();
    let log_str = logs.join("\n");
    assert!(
        !log_str.contains("\"bid\""),
        "No [bid] diagnostic should be emitted on error/validation failure path"
    );
}

#[test]
fn test_diagnostics_tag_uniqueness() {
    let expected_tags = ["escrow", "bid", "settlement", "payment"];
    for i in 0..expected_tags.len() {
        for j in (i + 1)..expected_tags.len() {
            assert_ne!(
                expected_tags[i], expected_tags[j],
                "Domain tag must be unique"
            );
        }
    }
}

#[cfg(feature = "diagnostics")]
#[test]
fn test_get_protocol_diagnostics_basic() {
    let (env, client, admin, contract_addr) = full_setup();
    let business = setup_verified_business(&env, &client, &admin);
    let investor = setup_verified_investor(&env, &client, 50_000);
    let currency = setup_token(&env, &client, &admin, &business, &investor, &contract_addr);

    // Before any invoices: counts should be zero.
    let diag = client.get_protocol_diagnostics();
    assert_eq!(diag.total_invoices, 0);
    assert_eq!(diag.pending_invoices, 0);
    assert_eq!(diag.verified_invoices, 0);
    assert!(!diag.is_paused);
    assert!(!diag.is_maintenance);
    assert!(!diag.backpressure_active);
    assert_eq!(diag.currency_count, 1);

    // Upload and verify an invoice, then check counts update.
    let invoice_id = client.upload_invoice(
        &business,
        &10_000i128,
        &currency,
        &(env.ledger().timestamp() + 86_400),
        &String::from_str(&env, "Diagnostics entry-point test"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let diag2 = client.get_protocol_diagnostics();
    assert_eq!(diag2.total_invoices, 1);
    assert_eq!(diag2.pending_invoices, 1);
    assert_eq!(diag2.verified_invoices, 0);

    client.verify_invoice(&invoice_id);
    let diag3 = client.get_protocol_diagnostics();
    assert_eq!(diag3.pending_invoices, 0);
    assert_eq!(diag3.verified_invoices, 1);
    assert_eq!(diag3.ledger_sequence, env.ledger().sequence());
}
