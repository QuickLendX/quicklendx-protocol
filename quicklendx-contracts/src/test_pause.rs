#![cfg(test)]

use crate::emergency::DEFAULT_EMERGENCY_TIMELOCK_SECS;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke};
use soroban_sdk::{token, Address, Env, IntoVal, String, Vec};

/// Standard test setup: registers contract, initializes admin, generates test addresses.
pub fn setup_contract_with_admin() -> (Env, QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    client.initialize_admin(&admin);
    (env, client, admin, business)
}

fn setup(
    env: &Env,
) -> (
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    let currency = Address::generate(env);
    client.initialize_admin(&admin);
    (client, admin, business, investor, currency)
}

fn submit_business_kyc(env: &Env, client: &QuickLendXContractClient, business: &Address) {
    client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
}

fn submit_investor_kyc(env: &Env, client: &QuickLendXContractClient, investor: &Address) {
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
}

fn verify_investor_for_test(
    env: &Env,
    client: &QuickLendXContractClient,
    investor: &Address,
    limit: i128,
) {
    submit_investor_kyc(env, client, investor);
    client.verify_investor(investor, &limit);
}

// ============================================================================
// Core pause/unpause behavior
// ============================================================================

#[test]
fn test_pause_blocks_user_and_invoice_state_mutations() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

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
    assert!(client.is_paused());

    // store_invoice blocked
    let result = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);

    // verify_invoice blocked
    let result = client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_allows_governance_configuration_updates() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.pause(&admin);

    // Admin config functions remain allowed during pause
    assert_eq!(client.set_bid_ttl_days(&14), 14);

    client.add_currency(&admin, &currency);
    assert!(client.is_allowed_currency(&currency));

    client.update_protocol_limits(&admin, &25i128, &45u64, &3_600u64);

    client.unpause(&admin);

    // Updated limits affect post-unpause operations
    let result = client.try_store_invoice(
        &business,
        &24i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Below min"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::InvalidAmount);
}

#[test]
fn test_pause_allows_kyc_review_admin_operations() {
    let env = Env::default();
    let (client, admin, business, investor, _currency) = setup(&env);

    submit_business_kyc(&env, &client, &business);
    submit_investor_kyc(&env, &client, &investor);

    client.pause(&admin);

    client.verify_business(&admin, &business);
    client.verify_investor(&investor, &1_500i128);

    let business_status = client
        .get_business_verification_status(&business)
        .expect("business verification");
    let investor_status = client
        .get_investor_verification(&investor)
        .expect("investor verification");

    assert!(matches!(
        business_status.status,
        crate::verification::BusinessVerificationStatus::Verified
    ));
    assert!(matches!(
        investor_status.status,
        crate::verification::BusinessVerificationStatus::Verified
    ));
}

#[test]
fn test_pause_allows_admin_rotation_and_new_admin_unpause() {
    let env = Env::default();
    let (client, admin, _business, _investor, _currency) = setup(&env);
    let new_admin = Address::generate(&env);

    client.pause(&admin);
    client.transfer_admin(&new_admin);
    assert_eq!(client.get_current_admin(), Some(new_admin.clone()));

    // Old admin cannot unpause
    let result = client.try_unpause(&admin);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::NotAdmin);

    client.unpause(&new_admin);
    assert!(!client.is_paused());
}

#[test]
fn test_pause_allows_emergency_withdraw_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);

    let token_admin = Address::generate(&env);
    let token_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let token_client = token::Client::new(&env, &token_id);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);
    let target = Address::generate(&env);
    let amount = 1_000i128;

    token_admin_client.mint(&contract_id, &amount);
    client.pause(&admin);

    client.initiate_emergency_withdraw(&admin, &token_id, &amount, &target);
    assert!(client.get_pending_emergency_withdraw().is_some());

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_EMERGENCY_TIMELOCK_SECS + 1);
    client.execute_emergency_withdraw(&admin);

    assert_eq!(token_client.balance(&target), amount);
    assert!(client.get_pending_emergency_withdraw().is_none());
    assert!(client.is_paused());
}

// ============================================================================
// Bid and escrow flows blocked during pause
// ============================================================================

#[test]
fn test_pause_blocks_accept_bid_and_fund() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);

    let result = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_release_escrow_funds() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    let accept_res = client.try_accept_bid(&invoice_id, &bid_id);
    if let Err(err) = accept_res {
        panic!("Setup failed at accept_bid: {:?}", err);
    }

    client.pause(&admin);

    let result = client.try_release_escrow_funds(&invoice_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_refund_escrow_funds() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    client.accept_bid(&invoice_id, &bid_id);

    client.pause(&admin);

    let result = client.try_refund_escrow_funds(&invoice_id, &business);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_withdraw_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);

    let result = client.try_withdraw_bid(&bid_id);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

// ============================================================================
// Invoice management blocked during pause
// ============================================================================

#[test]
fn test_pause_blocks_update_invoice_category() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);

    let result = client.try_update_invoice_category(&invoice_id, &InvoiceCategory::Products);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_settle_invoice() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let _bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);

    client.pause(&admin);

    let result = client.try_settle_invoice(&invoice_id, &1000i128);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_add_investment_insurance() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let _bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1100i128);
    client.accept_bid_and_fund(&invoice_id, &_bid_id);
    client.release_escrow_funds(&invoice_id);

    let investment = client.get_invoice_investment(&invoice_id);
    let provider = Address::generate(&env);

    client.pause(&admin);

    let result = client.try_add_investment_insurance(&investment.investment_id, &provider, &80);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

// ============================================================================
// KYC and user onboarding blocked during pause
// ============================================================================

#[test]
fn test_pause_and_unpause_require_admin_signature() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);

    client.mock_all_auths().initialize_admin(&admin);

    let spoofed_pause = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };
    let pause_result = client.mock_auths(&[spoofed_pause]).try_pause(&admin);
    let pause_err = pause_result
        .err()
        .expect("spoofed pause must fail")
        .err()
        .expect("spoofed pause must abort at auth");
    assert_eq!(pause_err, soroban_sdk::InvokeError::Abort);
    assert!(!client.is_paused(), "failed spoofed pause must not mutate state");

    client.pause(&admin);
    assert!(client.is_paused());

    let spoofed_unpause = MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "unpause",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    };
    let unpause_result = client.mock_auths(&[spoofed_unpause]).try_unpause(&admin);
    let unpause_err = unpause_result
        .err()
        .expect("spoofed unpause must fail")
        .err()
        .expect("spoofed unpause must abort at auth");
    assert_eq!(unpause_err, soroban_sdk::InvokeError::Abort);
    assert!(client.is_paused(), "failed spoofed unpause must leave pause flag set");
}

#[test]
fn test_pause_blocks_kyc_submission() {
    let env = Env::default();
    let (client, admin, business, _investor, _currency) = setup(&env);

    client.pause(&admin);

    let result = client.try_submit_kyc_application(&business, &String::from_str(&env, "Data"));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_investor_kyc_submission() {
    let env = Env::default();
    let (client, admin, _business, investor, _currency) = setup(&env);

    client.pause(&admin);

    let result = client.try_submit_investor_kyc(&investor, &String::from_str(&env, "Data"));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

// ============================================================================
// Tag management blocked during pause
// ============================================================================

#[test]
fn test_pause_blocks_tag_management() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86400;

    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    client.pause(&admin);

    let result = client.try_add_invoice_tag(&invoice_id, &String::from_str(&env, "Urgent"));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);

    let result = client.try_remove_invoice_tag(&invoice_id, &String::from_str(&env, "Urgent"));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

// ============================================================================
// Query functions always allowed
// ============================================================================

#[test]
fn test_pause_allows_all_query_functions() {
    let env = Env::default();
    let (client, admin, business, _investor, _currency) = setup(&env);

    client.pause(&admin);

    // All read-only operations must succeed while paused
    client.get_current_admin();
    client.is_paused();
    client.get_bid_ttl_days();
    client.get_total_invoice_count();
    client.get_available_invoices();
    client.get_invoice_by_business(&business);
    client.get_platform_fee();
    client.get_pending_businesses();
    client.get_verified_businesses();
    client.get_pending_investors();
    client.get_verified_investors();
}

// ============================================================================
// Determinism: repeated pause/unpause cycles
// ============================================================================

#[test]
fn test_pause_unpause_cycle_is_deterministic() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    for _ in 0..3 {
        assert!(!client.is_paused());

        // Operation succeeds when unpaused
        let _ = client.store_invoice(
            &business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );

        client.pause(&admin);
        assert!(client.is_paused());

        // Operation fails when paused
        let result = client.try_store_invoice(
            &business,
            &1_000i128,
            &currency,
            &due_date,
            &String::from_str(&env, "Blocked"),
            &InvoiceCategory::Services,
            &Vec::new(&env),
        );
        assert!(result.is_err());

        client.unpause(&admin);
    }
}

// ============================================================================
// Full mutating-entrypoint coverage matrix
//
// The emergency circuit breaker is only as strong as its weakest entrypoint:
// a single mutating path that ignores the pause flag lets value move while the
// protocol is supposed to be frozen. The tests below enumerate every
// state-mutating entrypoint named in the pause matrix — store_invoice,
// place_bid, accept_bid_and_fund, process_partial_payment, settle_invoice —
// and assert two properties for each:
//
//   1. Blocked: while paused, the call rejects with `ContractPaused` and no
//      state is mutated.
//   2. Recovery: after unpause, the same call succeeds, proving the breaker is
//      fully reversible.
//
// `process_partial_payment` previously lacked a `require_not_paused` guard;
// `test_pause_blocks_process_partial_payment` is the regression test for that
// fix. See docs/contracts/operations.md for the authoritative pause matrix.
// ============================================================================

/// Token-backed fixture that drives an invoice to `Funded` status so the
/// payment and settlement entrypoints can be exercised end to end.
///
/// Funding and settlement move real value (`accept_bid_and_fund` pulls the bid
/// into escrow; `settle_invoice` / a completing `process_partial_payment`
/// release escrow and disburse returns), so these paths require a registered
/// Stellar Asset Contract with funded, pre-approved balances — a
/// `generate()`-only currency is not enough. Modeled on the e2e fixture in
/// tests/invoice_lifecycle_e2e.rs.
///
/// Returns `(client, admin, invoice_id)` with the invoice in `Funded` status.
fn fund_invoice(
    env: &Env,
) -> (
    QuickLendXContractClient<'static>,
    Address,
    soroban_sdk::BytesN<32>,
) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    client.initialize_admin(&admin);

    // Real SAC token with funded, pre-approved balances.
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(&business, &20_000i128);
    sac.mint(&investor, &15_000i128);
    sac.mint(&contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(&business, &contract_id, &20_000i128, &exp);
    tok.approve(&investor, &contract_id, &15_000i128, &exp);
    client.add_currency(&admin, &currency);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Funded invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(env, &client, &investor, 15_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_000i128);
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    (client, admin, invoice_id)
}

#[test]
fn test_pause_blocks_place_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    client.pause(&admin);

    let result = client.try_place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

#[test]
fn test_pause_blocks_process_partial_payment() {
    // Regression: process_partial_payment must honor the pause flag. Before the
    // guard was added this call mutated payment state while paused.
    let env = Env::default();
    let (client, admin, invoice_id) = fund_invoice(&env);

    client.pause(&admin);

    let result = client.try_process_partial_payment(
        &invoice_id,
        &400i128,
        &String::from_str(&env, "tx-blocked"),
    );
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);

    // No payment was recorded while paused.
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 0);
}

#[test]
fn test_pause_blocks_make_payment_alias() {
    // make_payment delegates to process_partial_payment and must also be gated.
    let env = Env::default();
    let (client, admin, invoice_id) = fund_invoice(&env);

    client.pause(&admin);

    let result =
        client.try_make_payment(&invoice_id, &400i128, &String::from_str(&env, "tx-alias"));
    assert!(result.is_err());
    let err = result.unwrap_err().unwrap();
    assert_eq!(err, QuickLendXError::ContractPaused);
}

// ============================================================================
// Unpause recovery: each mutating entrypoint resumes after the breaker clears
// ============================================================================

#[test]
fn test_unpause_restores_store_invoice() {
    let env = Env::default();
    let (client, admin, business, _investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    client.pause(&admin);
    let blocked = client.try_store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Blocked"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    assert_eq!(blocked.unwrap_err().unwrap(), QuickLendXError::ContractPaused);

    client.unpause(&admin);
    assert!(!client.is_paused());

    // Same call now succeeds.
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "After unpause"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.amount, 1_000i128);
}

#[test]
fn test_unpause_restores_place_bid() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);

    client.pause(&admin);
    let blocked = client.try_place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    assert_eq!(blocked.unwrap_err().unwrap(), QuickLendXError::ContractPaused);

    client.unpause(&admin);

    // Same call now succeeds and returns a stored bid id.
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);
    assert!(client.get_bid(&bid_id).is_some());
}

#[test]
fn test_unpause_restores_accept_bid_and_fund() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);
    let due_date = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(&env, &client, &investor, 10_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_100i128);

    client.pause(&admin);
    let blocked = client.try_accept_bid_and_fund(&invoice_id, &bid_id);
    assert_eq!(blocked.unwrap_err().unwrap(), QuickLendXError::ContractPaused);

    client.unpause(&admin);

    // Same call now succeeds; invoice transitions to Funded.
    client.accept_bid_and_fund(&invoice_id, &bid_id);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Funded);
}

#[test]
fn test_unpause_restores_process_partial_payment() {
    let env = Env::default();
    let (client, admin, business, investor, currency) = setup(&env);

    let invoice_id = fund_invoice(&env, &client, &business, &investor, &currency);

    client.pause(&admin);
    let blocked = client.try_process_partial_payment(
        &invoice_id,
        &400i128,
        &String::from_str(&env, "tx-blocked"),
    );
    assert_eq!(blocked.unwrap_err().unwrap(), QuickLendXError::ContractPaused);

    client.unpause(&admin);

    // Same call now succeeds and records the payment.
    client.process_partial_payment(&invoice_id, &400i128, &String::from_str(&env, "tx-ok"));
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 400i128);
}

/// Token-backed fixture for tests that drive payment/settlement to completion.
///
/// Settlement releases escrow and disburses returns via real `transfer_funds`
/// calls, so those paths require a registered Stellar Asset Contract with
/// funded balances — a `generate()`-only currency is not enough. Modeled on the
/// e2e fixture in tests/invoice_lifecycle_e2e.rs.
///
/// Returns a fully-funded (`Funded` status) invoice ready for payment.
fn setup_funded_with_token(
    env: &Env,
) -> (
    QuickLendXContractClient<'static>,
    Address,
    soroban_sdk::BytesN<32>,
) {
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);
    client.initialize_admin(&admin);

    // Real SAC token with funded, pre-approved balances.
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(env, &currency);
    let tok = token::Client::new(env, &currency);
    sac.mint(&business, &20_000i128);
    sac.mint(&investor, &15_000i128);
    sac.mint(&contract_id, &1i128);
    let exp = env.ledger().sequence() + 100_000;
    tok.approve(&business, &contract_id, &20_000i128, &exp);
    tok.approve(&investor, &contract_id, &15_000i128, &exp);
    client.add_currency(&admin, &currency);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &1_000i128,
        &currency,
        &due_date,
        &String::from_str(env, "Token-funded invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);
    verify_investor_for_test(env, &client, &investor, 15_000);
    let bid_id = client.place_bid(&investor, &invoice_id, &1_000i128, &1_000i128);
    // accept_bid_and_fund moves the bid into escrow so settlement can release it.
    client.accept_bid_and_fund(&invoice_id, &bid_id);

    (client, admin, invoice_id)
}

#[test]
fn test_unpause_restores_settle_invoice() {
    let env = Env::default();
    let (client, admin, invoice_id) = setup_funded_with_token(&env);

    client.pause(&admin);
    let blocked = client.try_settle_invoice(&invoice_id, &1_000i128);
    assert_eq!(blocked.unwrap_err().unwrap(), QuickLendXError::ContractPaused);

    client.unpause(&admin);

    // Same call now succeeds; invoice reaches a terminal Paid state.
    client.settle_invoice(&invoice_id, &1_000i128);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Paid);
}

// ============================================================================
// Mid-lifecycle pause: a breaker engaged after funding still freezes payments
// ============================================================================

#[test]
fn test_pause_mid_lifecycle_freezes_then_resumes_payment() {
    let env = Env::default();
    let (client, admin, invoice_id) = setup_funded_with_token(&env);

    // Take one partial payment while operating normally.
    client.process_partial_payment(&invoice_id, &400i128, &String::from_str(&env, "tx-1"));
    assert_eq!(client.get_invoice(&invoice_id).total_paid, 400i128);

    // Breaker engaged mid-lifecycle: the next payment is rejected and state is
    // unchanged. Reads still work.
    client.pause(&admin);
    let blocked =
        client.try_process_partial_payment(&invoice_id, &600i128, &String::from_str(&env, "tx-2"));
    assert_eq!(blocked.unwrap_err().unwrap(), QuickLendXError::ContractPaused);
    assert_eq!(client.get_invoice(&invoice_id).total_paid, 400i128);

    // Recovery: the remaining payment completes the lifecycle (triggers settle).
    client.unpause(&admin);
    client.process_partial_payment(&invoice_id, &600i128, &String::from_str(&env, "tx-2"));
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 1_000i128);
    assert_eq!(invoice.status, crate::invoice::InvoiceStatus::Paid);
}

